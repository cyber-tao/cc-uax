mod anim;
mod gameplay;
mod graph;
mod material;
mod math;
mod mesh_cloth;
mod niagara;
mod pcg;
mod scalar;
mod sequencer;
mod state_tree;

use crate::property::{ParseCtx, PropertyParseStatus};
use crate::reader::Reader;
use crate::structured_value::Value;
use anyhow::{Result, bail};

/// Rejects a native struct whose payload is really tagged properties unless those
/// properties parsed cleanly and consumed the declared value window exactly.
///
/// A native decoder must consume exactly its payload; accepting a short or
/// malformed tagged block here would let the surrounding property loop resume at
/// the wrong offset.
pub(crate) fn ensure_complete_tagged_payload(
    r: &Reader,
    value_end: u64,
    status: &PropertyParseStatus,
    name: &str,
) -> Result<()> {
    if matches!(
        status,
        PropertyParseStatus::NonTaggedPayload | PropertyParseStatus::FailedAfterEntries
    ) {
        bail!("{name} tagged payload is malformed ({})", status.as_str());
    }
    if r.pos() != value_end {
        bail!(
            "{name} tagged payload ended at byte {}, expected {value_end}",
            r.pos()
        );
    }
    Ok(())
}

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
            // FVMExternalFunctionBindingInfo::Serialize and FAnimSyncMarker::Serialize
            // both call SerializeTaggedProperties, so their payload is tagged properties.
            | "VMExternalFunctionBindingInfo"
            | "AnimSyncMarker"
            | "NiagaraVariant"
            | "StateTreeStateLink"
            | "MetaSoundFrontendGraphComment"
            // These serializers only register custom versions and return false,
            // so the actual payload remains ordinary tagged properties.
            | "StateTreeReference"
            | "PCGAttributePropertySelector"
            | "PCGAttributePropertyInputSelector"
            | "PCGAttributePropertyOutputNoSourceSelector"
            | "PCGAttributePropertyOutputSelector"
            // FTransform is the one core math type whose USTRUCT is not
            // `immutable` and whose TTransformStructOpsTypeTraits leaves
            // `WithSerializer` commented out (UE5.0-5.8), so UE writes a tagged
            // Rotation/Translation/Scale3D block. FTransform3f/FTransform3d are
            // immutable and keep their binary layout.
            | "Transform"
    )
}

pub(crate) fn parse_native_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    if let Some(v) = anim::parse_anim_struct(r, name, ctx)? {
        return Ok(Some(v));
    }
    if let Some(v) = math::parse_math_struct(r, name, ctx)? {
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
    if let Some(v) = state_tree::parse_state_tree_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    if let Some(v) = mesh_cloth::parse_mesh_cloth_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    if let Some(v) = pcg::parse_pcg_struct(r, name, ctx, value_end)? {
        return Ok(Some(v));
    }
    niagara::parse_niagara_struct(r, name, ctx, value_end)
}
