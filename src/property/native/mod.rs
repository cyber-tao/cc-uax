mod gameplay;
mod graph;
mod material;
mod math;
mod mesh_cloth;
mod niagara;
mod scalar;
mod sequencer;

use crate::property::ParseCtx;
use crate::reader::Reader;
use anyhow::Result;
use serde_json::Value;

pub(crate) fn is_tagged_fallback_struct(name: &str) -> bool {
    matches!(
        name,
        "ConstraintInstance"
            | "Timeline"
            | "AnimNotifyEvent"
            | "PostProcessSettings"
            | "HierarchicalSimplification"
            // FAlphaBlend / FAnimCurveBase-derived curves declare WithSerializer but
            // their Serialize returns false, so the payload is tagged properties.
            | "AlphaBlend"
            | "FloatCurve"
            | "TransformCurve"
            | "VectorCurve"
            // FGameplayEffectModifierMagnitude::Serialize also returns false; the
            // landscape per-layer struct has no custom serializer (the enclosing map
            // carries the native flag), so both are tagged-property payloads.
            | "GameplayEffectModifierMagnitude"
            | "LandscapeLayerComponentData"
            // FVMExternalFunctionBindingInfo::Serialize calls SerializeTaggedProperties.
            | "VMExternalFunctionBindingInfo"
    )
}

pub(crate) fn parse_native_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    if let Some(v) = math::parse_math_struct(r, name)? {
        return Ok(Some(v));
    }
    if let Some(v) = scalar::parse_scalar_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    if let Some(v) = material::parse_material_input_struct(r, name, ctx)? {
        return Ok(Some(v));
    }
    if let Some(v) = sequencer::parse_sequencer_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    if let Some(v) = graph::parse_graph_pin_struct(r, name, ctx)? {
        return Ok(Some(v));
    }
    if let Some(v) = gameplay::parse_gameplay_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    if let Some(v) = mesh_cloth::parse_mesh_cloth_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    niagara::parse_niagara_struct(r, name, ctx, value_end)
}
