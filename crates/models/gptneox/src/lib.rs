//! An implementation of [GPT-NeoX](https://huggingface.co/docs/transformers/model_doc/gpt_neox) for the `llm` ecosystem.
//! This crate also supports the [RedPajama](https://www.together.xyz/blog/redpajama) GPT-NeoX model.
#![deny(missing_docs)]

use std::error::Error;

use ggml::Tensor;
use llm_base::{
    ggml,
    model::{common, HyperparametersWriteError},
    util, FileType, GraphOutputs, InferenceSession, InferenceSessionConfig, KnownModel, LoadError,
    ModelContext, ModelParameters, OutputRequest, Regex, TensorLoader, TokenId, Tokenizer,
};

/// The GPT-NeoX model. Ref: [GitHub](https://github.com/EleutherAI/gpt-neox)
///
/// # Safety
/// This implements [Send] and [Sync] as it is immutable after construction.
pub struct GptNeoX {
    params: ModelParameters,

    hyperparameters: Hyperparameters,
    tokenizer: Tokenizer,

    // model-global weights
    // normalization gain & bias
    ln_f_g: Tensor,
    ln_f_b: Tensor,
    // weight token embeddings
    wte: Tensor,
    // language model head gain
    lmh_g: Tensor,

    // weights for the model
    layers: Vec<Layer>,

    // must be kept alive for the model
    context: ModelContext,
}

unsafe impl Send for GptNeoX {}
unsafe impl Sync for GptNeoX {}

impl KnownModel for GptNeoX {
    type Hyperparameters = Hyperparameters;

    fn new<E: Error>(
        hyperparameters: Hyperparameters,
        params: ModelParameters,
        tokenizer: Tokenizer,
        tensor_loader: impl TensorLoader<E>,
    ) -> Result<Self, E>
    where
        Self: Sized,
    {
        let mut tl = tensor_loader;

        // model-global weights
        let wte = tl.load("gpt_neox.embed_in.weight")?;

        let backend = params.backend(0);

        let ln_f_g = tl
            .load("gpt_neox.final_layer_norm.weight")?
            .transfer_to(backend);
        let ln_f_b = tl
            .load("gpt_neox.final_layer_norm.bias")?
            .transfer_to(backend);
        let lmh_g = tl.load("embed_out.weight")?.transfer_to(backend);

        let mut layers = Vec::new();
        for i in 0..hyperparameters.n_layer {
            let backend = params.backend(i);
            let layer = Layer {
                ln_1_g: tl
                    .load(&format!("gpt_neox.layers.{i}.input_layernorm.weight"))?
                    .transfer_to(backend),
                ln_1_b: tl
                    .load(&format!("gpt_neox.layers.{i}.input_layernorm.bias"))?
                    .transfer_to(backend),

                c_attn_attn_w: tl
                    .load(&format!(
                        "gpt_neox.layers.{i}.attention.query_key_value.weight"
                    ))?
                    .transfer_to(backend),
                c_attn_attn_b: tl
                    .load(&format!(
                        "gpt_neox.layers.{i}.attention.query_key_value.bias"
                    ))?
                    .transfer_to(backend),

                c_attn_proj_w: tl
                    .load(&format!("gpt_neox.layers.{i}.attention.dense.weight"))?
                    .transfer_to(backend),
                c_attn_proj_b: tl
                    .load(&format!("gpt_neox.layers.{i}.attention.dense.bias"))?
                    .transfer_to(backend),

                ln_2_g: tl
                    .load(&format!(
                        "gpt_neox.layers.{i}.post_attention_layernorm.weight"
                    ))?
                    .transfer_to(backend),
                ln_2_b: tl
                    .load(&format!(
                        "gpt_neox.layers.{i}.post_attention_layernorm.bias"
                    ))?
                    .transfer_to(backend),

                c_mlp_fc_w: tl
                    .load(&format!("gpt_neox.layers.{i}.mlp.dense_h_to_4h.weight"))?
                    .transfer_to(backend),
                c_mlp_fc_b: tl
                    .load(&format!("gpt_neox.layers.{i}.mlp.dense_h_to_4h.bias"))?
                    .transfer_to(backend),

                c_mlp_proj_w: tl
                    .load(&format!("gpt_neox.layers.{i}.mlp.dense_4h_to_h.weight"))?
                    .transfer_to(backend),
                c_mlp_proj_b: tl
                    .load(&format!("gpt_neox.layers.{i}.mlp.dense_4h_to_h.bias"))?
                    .transfer_to(backend),
            };

            layers.push(layer);
        }

        let context = tl.finish();

        Ok(GptNeoX {
            hyperparameters,
            params,
            tokenizer,
            ln_f_g,
            ln_f_b,
            wte,
            lmh_g,
            layers,
            context,
        })
    }

    fn start_session(&self, config: InferenceSessionConfig) -> InferenceSession {
        InferenceSession::new(
            config,
            &self.params,
            self.hyperparameters.n_layer,
            self.hyperparameters.n_embd,
            self.hyperparameters.n_vocab,
        )
    }

    // allow snake case here as its a one-to-one mapping of the original names
    #[allow(non_snake_case)]
    fn evaluate(
        &self,
        session: &mut InferenceSession,
        input_tokens: &[TokenId],
        output_request: &mut OutputRequest,
    ) {
        let n = input_tokens.len();
        let n_past = session.n_past;
        let n_ctx = self.params.context_size;

        let Hyperparameters {
            n_embd,
            n_head,
            n_vocab,
            n_layer,
            n_rot,
            use_parallel_residual,
            ..
        } = self.hyperparameters;

        let outputs = session.compute(self.context.clone(), input_tokens, |builder| {
            let mut ctx0 = builder.ctx0.borrow_mut();
            let embd = builder.embd;
            let mut input_layer = ctx0.op_get_rows(&self.wte, embd);
            let (memory_k_size, memory_v_size) = (
                builder.memory_k.element_size(),
                builder.memory_v.element_size(),
            );

            let mut gf = ctx0.create_compute_graph();

            for il in 0..n_layer {
                ctx0.set_offloading(self.params.should_offload(il));
                // attention uses first scratch buffer
                ctx0.use_scratch(builder.get_scratch(0));

                // self-attention
                let mut current = ctx0.op_norm(&input_layer);
                current = ctx0.op_add(
                    &ctx0.op_mul(&current, &self.layers[il].ln_1_g),
                    &self.layers[il].ln_1_b,
                );

                // self-attention compute QKV
                current = ctx0.op_mul_mat(&self.layers[il].c_attn_attn_w, &current);
                current = ctx0.op_add(&current, &self.layers[il].c_attn_attn_b);

                let nb = current.get_nb()[1];
                let f32_size = std::mem::size_of::<f32>();

                let mut qcur = ctx0.op_cont(&ctx0.op_view_3d(
                    &current,
                    (n_embd / n_head, n_head, n),
                    (nb / n_head, nb),
                    0,
                ));
                let mut kcur = ctx0.op_cont(&ctx0.op_view_3d(
                    &current,
                    (n_embd / n_head, n_head, n),
                    (nb / n_head, nb),
                    f32_size * n_embd / n_head,
                ));
                let mut vcur = ctx0.op_cont(&ctx0.op_view_3d(
                    &current,
                    (n_embd / n_head, n_head, n),
                    (nb / n_head, nb),
                    2 * f32_size * n_embd / n_head,
                ));

                // self-attention using mode = 2 for GPT-NeoX mode
                let overrides = self.params.rope_overrides.as_ref();
                qcur = ctx0.op_rope_inplace(&qcur, n_past, n_rot, 2, overrides);
                kcur = ctx0.op_rope_inplace(&kcur, n_past, n_rot, 2, overrides);

                // store key and value to memory
                vcur = ctx0.op_transpose(&ctx0.op_reshape_2d(&vcur, n_embd, n));

                let k = ctx0.op_view_1d(
                    builder.memory_k,
                    n * n_embd,
                    (memory_k_size * n_embd) * (il * n_ctx + n_past),
                );

                let v = ctx0.op_view_2d(
                    builder.memory_v,
                    (n, n_embd),
                    n_ctx * memory_v_size,
                    (il * n_ctx) * memory_v_size * n_embd + n_past * memory_v_size,
                );

                gf.build_forward_expand(&ctx0.op_cpy(&kcur, &k));
                gf.build_forward_expand(&ctx0.op_cpy(&vcur, &v));

                // Q = Qcur.contiguous().view(n_embd/n_head, n_head, N).permute(0, 2, 1, 3)
                let Q = ctx0.op_permute(&qcur, (0, 2, 1, 3));
                // K = Kmem.view(n_embd/n_head, n_head, n_past + N).permute(0, 2, 1, 3)
                let K = ctx0.op_permute(
                    &ctx0.op_reshape_3d(
                        &ctx0.op_view_1d(
                            builder.memory_k,
                            (n_past + n) * n_embd,
                            il * n_ctx * memory_k_size * n_embd,
                        ),
                        n_embd / n_head,
                        n_head,
                        n_past + n,
                    ),
                    (0, 2, 1, 3),
                );

                // K * Q
                let KQ = ctx0.op_mul_mat(&K, &Q);

                // KQ_scaled = KQ / sqrt(n_embd/n_head)
                let KQ_scaled = ctx0.op_scale_inplace(
                    &KQ,
                    &ctx0.new_f32(1f32 / f32::sqrt(n_embd as f32 / n_head as f32)),
                );

                // KQ_masked = mask_past(KQ_scaled)
                let KQ_masked = ctx0.op_diag_mask_inf_inplace(&KQ_scaled, n_past);

                // KQ = soft_max(KQ_masked)
                let KQ_softmax = ctx0.op_soft_max_inplace(&KQ_masked);

                // V_trans = Vmem.view(n_embd/n_head, n_head, n_past + N).permute(1, 2, 0, 3).contiguous()
                let V = ctx0.op_view_3d(
                    builder.memory_v,
                    (n_past + n, n_embd / n_head, n_head),
                    (
                        n_ctx * memory_v_size,
                        n_ctx * memory_v_size * n_embd / n_head,
                    ),
                    il * n_ctx * memory_v_size * n_embd,
                );

                // KQV = transpose(V) * KQ_soft_max
                let KQV = ctx0.op_mul_mat(&V, &KQ_softmax);
                // KQV_merged = KQV.permute(0, 2, 1, 3)
                let KQV_merged = ctx0.op_permute(&KQV, (0, 2, 1, 3));

                // cur = KQV_merged.contiguous().view(n_embd, N)
                current = ctx0.op_cpy(&KQV_merged, &ctx0.new_tensor_2d(ggml::Type::F32, n_embd, n));

                // self-attention projection
                current = ctx0.op_mul_mat(&self.layers[il].c_attn_proj_w, &current);
                current = ctx0.op_add(&current, &self.layers[il].c_attn_proj_b);

                // use the second scratch for the feed forward
                ctx0.use_scratch(builder.get_scratch(1));

                let feedforward_input: Tensor;
                if !use_parallel_residual {
                    feedforward_input = ctx0.op_add(&current, &input_layer);
                    current = feed_forward_network(&ctx0, &self.layers[il], &feedforward_input);
                    // input for next layer
                    input_layer = ctx0.op_add(&current, &feedforward_input);
                } else {
                    // calculate with parallel residual
                    feedforward_input = current.share();

                    // this is independent of the self-attention result, so it could be done in parallel to the self-attention
                    // note here we pass inpL instead of cur
                    current = feed_forward_network(&ctx0, &self.layers[il], &input_layer);

                    // layer input + FF
                    current = ctx0.op_add(&current, &feedforward_input);

                    // input for next layer
                    input_layer = ctx0.op_add(&current, &input_layer);
                }
            }

            // use the first scratch for the norm
            ctx0.use_scratch(builder.get_scratch(0));

            // normalize the output
            input_layer = ctx0.op_norm(&input_layer);
            // inpL = ln_f_g*inpL + ln_f_b
            input_layer = ctx0.op_add(&ctx0.op_mul(&input_layer, &self.ln_f_g), &self.ln_f_b);

            let embeddings_tensor: ggml::Tensor = input_layer.share();

            // Disable the scratchbuffer
            ctx0.use_scratch(None);
            ctx0.set_offloading(false);
            // apply language model head
            input_layer = ctx0.op_mul_mat(&self.lmh_g, &input_layer);

            (
                gf,
                GraphOutputs {
                    result: input_layer,
                    embedding_result: embeddings_tensor,
                },
            )
        });

        // finish evaluation
        common::read_last_token(session, &outputs.result, n_vocab, n);
        common::extract_logits(output_request, &outputs.result, n_vocab, n);
        common::extract_embeddings(output_request, &outputs.embedding_result, n_embd, n);
    }

    fn hyperparameters(&self) -> &Self::Hyperparameters {
        &self.hyperparameters
    }

    fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    fn context_size(&self) -> usize {
        self.params.context_size
    }

    fn bot_token_id(&self) -> Option<TokenId> {
        None
    }

    fn eot_token_id(&self) -> TokenId {
        self.tokenizer.id("<|endoftext|>".as_bytes()).unwrap()
    }

    fn quantize_tensors() -> Vec<Regex> {
        vec![Regex::new(".*weight").unwrap()]
    }

    fn skip_quantize_tensors() -> Vec<Regex> {
        vec![]
    }

    fn supports_rewind(&self) -> bool {
        true
    }
}

/// GPT-NeoX [hyperparameters](https://en.wikipedia.org/wiki/Hyperparameter_(machine_learning))
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Hyperparameters {
    /// Size of the model's vocabulary
    pub n_vocab: usize,
    /// Size of the model's context
    pub n_ctx: usize,
    /// Size of the model's embedding layer
    pub n_embd: usize,
    /// n_head
    pub n_head: usize,
    /// Number of layers in the model
    pub n_layer: usize,
    /// n_rot
    pub n_rot: usize,
    /// Whether to use a "parallel" formulation in each Transformer layer.
    /// This is on for most models, but is off for some e.g. RedPajama.
    pub use_parallel_residual: bool,
    /// file_type
    pub file_type: FileType,
}

impl Default for Hyperparameters {
    fn default() -> Self {
        Self {
            n_vocab: Default::default(),
            n_ctx: Default::default(),
            n_embd: Default::default(),
            n_head: Default::default(),
            n_layer: Default::default(),
            n_rot: Default::default(),
            file_type: Default::default(),
            use_parallel_residual: true,
        }
    }
}

impl llm_base::Hyperparameters for Hyperparameters {
    fn read_ggml(reader: &mut dyn std::io::BufRead) -> Result<Self, LoadError> {
        Ok(Hyperparameters {
            n_vocab: util::read_i32(reader)?.try_into()?,
            n_ctx: util::read_i32(reader)?.try_into()?,
            n_embd: util::read_i32(reader)?.try_into()?,
            n_head: util::read_i32(reader)?.try_into()?,
            n_layer: util::read_i32(reader)?.try_into()?,
            n_rot: util::read_i32(reader)?.try_into()?,
            use_parallel_residual: util::read_bool(reader)?,
            file_type: util::read_filetype(reader)?,
        })
    }

    fn write_ggml(&self, writer: &mut dyn std::io::Write) -> Result<(), HyperparametersWriteError> {
        util::write_i32(writer, self.n_vocab.try_into()?)?;
        util::write_i32(writer, self.n_ctx.try_into()?)?;
        util::write_i32(writer, self.n_embd.try_into()?)?;
        util::write_i32(writer, self.n_head.try_into()?)?;
        util::write_i32(writer, self.n_layer.try_into()?)?;
        util::write_i32(writer, self.n_rot.try_into()?)?;
        util::write_bool(writer, self.use_parallel_residual)?;
        util::write_i32(writer, self.file_type.into())?;
        Ok(())
    }

    fn n_vocabulary(&self) -> usize {
        self.n_vocab
    }

    fn file_type(&self) -> Option<FileType> {
        Some(self.file_type)
    }

    fn file_type_mut(&mut self) -> Option<&mut FileType> {
        Some(&mut self.file_type)
    }
}

struct Layer {
    // pre-normalization
    ln_1_g: Tensor,
    ln_1_b: Tensor,

    // attention
    c_attn_attn_w: Tensor,
    c_attn_attn_b: Tensor,

    c_attn_proj_w: Tensor,
    c_attn_proj_b: Tensor,

    // post normalization
    ln_2_g: Tensor,
    ln_2_b: Tensor,

    // feed-forward
    c_mlp_fc_w: Tensor,
    c_mlp_fc_b: Tensor,

    c_mlp_proj_w: Tensor,
    c_mlp_proj_b: Tensor,
}

fn feed_forward_network(context: &ggml::Context, layer: &Layer, input: &Tensor) -> Tensor {
    let mut current = context.op_norm(input);

    //gain and bias
    current = context.op_add(&context.op_mul(&current, &layer.ln_2_g), &layer.ln_2_b);

    // apply weights
    current = context.op_mul_mat(&layer.c_mlp_fc_w, &current);

    // apply bias
    current = context.op_add(&current, &layer.c_mlp_fc_b);

    // GELU activation
    current = context.op_gelu(&current);

    // projection
    // cur = proj_w*cur + proj_b
    current = context.op_mul_mat(&layer.c_mlp_proj_w, &current);

    current = context.op_add(&current, &layer.c_mlp_proj_b);

    current
}
