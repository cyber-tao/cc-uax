use crate::property::{ParseCtx, parse_soft_object};
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use anyhow::Result;

// Animation structs whose `Serialize` writes a fixed binary field layout
// (`Ar << Field`), as opposed to `SerializeTaggedProperties`.
pub(super) fn parse_anim_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
) -> Result<Option<Value>> {
    let v = match name {
        // FAnimationAttributeIdentifier::Serialize returns true and writes:
        //   Ar << Name << BoneName << BoneIndex << ScriptStructPath.
        "AnimationAttributeIdentifier" => {
            let name = ctx.names.resolve_raw(r.read_raw_name()?);
            let bone_name = ctx.names.resolve_raw(r.read_raw_name()?);
            let bone_index = r.read_i32()?;
            let script_struct_path = parse_soft_object(r, ctx)?;
            json!({
                "name": name,
                "bone_name": bone_name,
                "bone_index": bone_index,
                "script_struct_path": script_struct_path,
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}
