use std::sync::Arc;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use hf_hub::api::sync::Api;
use tokenizers::Tokenizer;
use tokio::sync::OnceCell;

use crate::error::{ContextForgeError, Result};

/// Trait for embedding providers — allows swapping models without changing consumers.
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector from text.
    fn embed_sync(&self, text: &str) -> Result<Vec<f32>>;

    /// Number of dimensions in the output vector.
    fn dimension(&self) -> usize;

    /// Identifier for the model (stored in DB for migration tracking).
    fn model_id(&self) -> &str;
}

/// Candle-based embedding provider using BERT-family models from HuggingFace.
pub struct CandleProvider {
    model: BertModel,
    tokenizer: Tokenizer,
    model_id: String,
    dimension: usize,
}

// SAFETY: BertModel contains Tensor fields that are Send when using Device::Cpu.
// candle_core::Tensor on CPU is backed by a Vec<u8> which is Send.
unsafe impl Send for CandleProvider {}
unsafe impl Sync for CandleProvider {}

impl CandleProvider {
    /// Load a sentence-transformer model from HuggingFace Hub (or cache).
    pub fn load(model_id: &str, dimension: usize) -> Result<Self> {
        let device = Device::Cpu;

        let api = Api::new()
            .map_err(|e| ContextForgeError::Embedding(format!("HF Hub API init failed: {e}")))?;
        let repo = api.model(model_id.to_string());

        let config_path = repo.get("config.json").map_err(|e| {
            ContextForgeError::Embedding(format!("Failed to download config.json: {e}"))
        })?;
        let tokenizer_path = repo.get("tokenizer.json").map_err(|e| {
            ContextForgeError::Embedding(format!("Failed to download tokenizer.json: {e}"))
        })?;
        let weights_path = repo.get("model.safetensors").map_err(|e| {
            ContextForgeError::Embedding(format!("Failed to download model.safetensors: {e}"))
        })?;

        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| ContextForgeError::Embedding(format!("Read config: {e}")))?;
        let config: BertConfig = serde_json::from_str(&config_str)
            .map_err(|e| ContextForgeError::Embedding(format!("Parse config: {e}")))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| ContextForgeError::Embedding(format!("Load tokenizer: {e}")))?;

        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device) }
                .map_err(|e| ContextForgeError::Embedding(format!("Load weights: {e}")))?;

        let model = BertModel::load(vb, &config)
            .map_err(|e| ContextForgeError::Embedding(format!("Build model: {e}")))?;

        tracing::info!("Loaded embedding model {model_id} ({dimension} dims)");

        Ok(Self {
            model,
            tokenizer,
            model_id: model_id.to_string(),
            dimension,
        })
    }

    /// Default: bge-small-en-v1.5 (384 dims, better quality than MiniLM, same speed).
    pub fn default_model() -> Result<Self> {
        Self::load("BAAI/bge-small-en-v1.5", 384)
    }

    /// Mean pooling with attention mask.
    fn mean_pooling(
        token_embeddings: &Tensor,
        attention_mask: &Tensor,
    ) -> candle_core::Result<Tensor> {
        let mask_expanded = attention_mask
            .unsqueeze(2)?
            .broadcast_as(token_embeddings.shape())?;

        let summed = (token_embeddings * &mask_expanded)?.sum(1)?;
        let mask_sum = mask_expanded.sum(1)?.clamp(1e-9, f64::MAX)?;

        &summed / &mask_sum
    }

    /// L2 normalize: x / ||x||_2
    fn l2_normalize(tensor: &Tensor) -> candle_core::Result<Tensor> {
        let norm = tensor
            .sqr()?
            .sum_keepdim(1)?
            .sqrt()?
            .clamp(1e-12, f64::MAX)?;

        tensor.broadcast_div(&norm)
    }
}

impl EmbeddingProvider for CandleProvider {
    fn embed_sync(&self, text: &str) -> Result<Vec<f32>> {
        let device = &self.model.device;

        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| ContextForgeError::Embedding(format!("Tokenize: {e}")))?;

        let token_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let token_type_ids = vec![0u32; token_ids.len()];

        let token_ids_t = Tensor::new(token_ids, device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| ContextForgeError::Embedding(format!("Tensor token_ids: {e}")))?;

        let attention_mask_t = Tensor::new(attention_mask, device)
            .and_then(|t| t.to_dtype(DType::F32))
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| ContextForgeError::Embedding(format!("Tensor attention_mask: {e}")))?;

        let token_type_ids_t = Tensor::new(&token_type_ids[..], device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| ContextForgeError::Embedding(format!("Tensor token_type_ids: {e}")))?;

        let output = self
            .model
            .forward(&token_ids_t, &token_type_ids_t, Some(&attention_mask_t))
            .map_err(|e| ContextForgeError::Embedding(format!("Forward pass: {e}")))?;

        let pooled = Self::mean_pooling(&output, &attention_mask_t)
            .map_err(|e| ContextForgeError::Embedding(format!("Mean pooling: {e}")))?;

        let normalized = Self::l2_normalize(&pooled)
            .map_err(|e| ContextForgeError::Embedding(format!("L2 normalize: {e}")))?;

        let embedding: Vec<f32> = normalized
            .squeeze(0)
            .and_then(|t| t.to_vec1())
            .map_err(|e| ContextForgeError::Embedding(format!("To vec: {e}")))?;

        debug_assert_eq!(embedding.len(), self.dimension);

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

/// Thread-safe lazy wrapper for any embedding provider.
#[derive(Clone)]
pub struct LazyEmbeddingEngine {
    inner: Arc<OnceCell<Box<dyn EmbeddingProvider>>>,
}

impl LazyEmbeddingEngine {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(OnceCell::new()),
        }
    }

    /// Get or initialize the engine, then embed the text.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let inner = self.inner.clone();
        inner
            .get_or_try_init(|| async {
                tokio::task::spawn_blocking(|| -> Result<Box<dyn EmbeddingProvider>> {
                    Ok(Box::new(CandleProvider::default_model()?))
                })
                .await
                .map_err(|e| ContextForgeError::Embedding(format!("Join error: {e}")))?
            })
            .await?;

        let text = text.to_string();
        let inner2 = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let engine = inner2.get().expect("engine initialized above");
            engine.embed_sync(&text)
        })
        .await
        .map_err(|e| ContextForgeError::Embedding(format!("Join error: {e}")))?
    }

    /// Get the model ID (returns None if engine not yet initialized).
    pub fn model_id(&self) -> Option<&str> {
        self.inner.get().map(|e| e.model_id())
    }

    /// Get the embedding dimension (returns None if engine not yet initialized).
    pub fn dimension(&self) -> Option<usize> {
        self.inner.get().map(|e| e.dimension())
    }
}

impl Default for LazyEmbeddingEngine {
    fn default() -> Self {
        Self::new()
    }
}
