use crate::property::ParseCtx;
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use crate::version::ue5;
use anyhow::Result;

fn large_world(ctx: &ParseCtx) -> bool {
    ctx.file_version_ue5 >= ue5::LARGE_WORLD_COORDINATES
}

fn read_coord(r: &mut Reader, lwc: bool) -> Result<f64> {
    if lwc {
        r.read_f64()
    } else {
        Ok(f64::from(r.read_f32()?))
    }
}

fn vec2(r: &mut Reader, lwc: bool) -> Result<Value> {
    Ok(json!({ "x": read_coord(r, lwc)?, "y": read_coord(r, lwc)? }))
}

fn vec3(r: &mut Reader, lwc: bool) -> Result<Value> {
    Ok(json!({
        "x": read_coord(r, lwc)?,
        "y": read_coord(r, lwc)?,
        "z": read_coord(r, lwc)?
    }))
}

fn vec4(r: &mut Reader, lwc: bool) -> Result<Value> {
    Ok(json!({
        "x": read_coord(r, lwc)?,
        "y": read_coord(r, lwc)?,
        "z": read_coord(r, lwc)?,
        "w": read_coord(r, lwc)?
    }))
}

pub(super) fn parse_math_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
) -> Result<Option<Value>> {
    let lwc = large_world(ctx);
    let v = match name {
        // Note: FVector_NetQuantize* subclasses only declare WithNetSerializer, so
        // their package payload is tagged properties — do not decode them natively.
        "Vector" => vec3(r, lwc)?,
        "Vector3f" => json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? }),
        "Vector2D" => vec2(r, lwc)?,
        "Vector2f" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "Vector4" => vec4(r, lwc)?,
        "Vector4f" => json!({
            "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
        }),
        "Rotator" => json!({
            "pitch": read_coord(r, lwc)?, "yaw": read_coord(r, lwc)?, "roll": read_coord(r, lwc)?
        }),
        "Rotator3f" => json!({
            "pitch": r.read_f32()?, "yaw": r.read_f32()?, "roll": r.read_f32()?
        }),
        "Quat" => vec4(r, lwc)?,
        "Quat4f" => json!({
            "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
        }),
        "IntPoint" | "Int32Point" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector" => json!({ "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()? }),
        "Guid" => json!(r.read_guid()?.to_hex()),
        "Color" => json!({
            "b": r.read_u8()?, "g": r.read_u8()?, "r": r.read_u8()?, "a": r.read_u8()?
        }),
        "LinearColor" => json!({
            "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
        }),
        "DateTime" | "Timespan" => json!(r.read_i64()?),
        "Transform" => {
            json!({
                "rotation": vec4(r, lwc)?,
                "translation": vec3(r, lwc)?,
                "scale3d": vec3(r, lwc)?
            })
        }
        "Transform3f" => {
            let rot = json!({
                "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
            });
            let trans = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            let scale = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
        }
        "Box" => {
            json!({
                "min": vec3(r, lwc)?,
                "max": vec3(r, lwc)?,
                "is_valid": r.read_u8()? != 0
            })
        }
        "Box3f" => {
            let min = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            let max = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "Box2D" => {
            json!({
                "min": vec2(r, lwc)?,
                "max": vec2(r, lwc)?,
                // TBox2::Serialize writes bIsValid as a single uint8 (not a 4-byte UBOOL).
                "is_valid": r.read_u8()? != 0
            })
        }
        "Box2f" => {
            let min = json!({ "x": r.read_f32()?, "y": r.read_f32()? });
            let max = json!({ "x": r.read_f32()?, "y": r.read_f32()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "FrameNumber" => json!({ "value": r.read_i32()? }),
        "Matrix" => {
            let mut m = Vec::with_capacity(16);
            for _ in 0..16 {
                m.push(json!(read_coord(r, lwc)?));
            }
            json!({ "m": m })
        }
        "Matrix44f" => {
            let mut m = Vec::with_capacity(16);
            for _ in 0..16 {
                m.push(json!(r.read_f32()? as f64));
            }
            json!({ "m": m })
        }
        // FrameRate deliberately has no arm: TStructOpsTypeTraits<FFrameRate> keeps
        // WithSerializer disabled (UE keeps the generic UPROPERTY layout for existing
        // assets), so a StructProperty(FrameRate) payload is tagged properties.
        // ScalarKind::FrameRate below still covers the genuinely native contexts
        // (PerPlatformFrameRate, MovieScene channel tick resolution).
        "IntVector2" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector4" => json!({
            "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()?, "w": r.read_i32()?
        }),
        "DeprecateSlateVector2D" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "RichCurveKey" => {
            let interp_mode = r.read_u8()?;
            let tangent_mode = r.read_u8()?;
            let tangent_weight_mode = r.read_u8()?;
            json!({
                "interp_mode": interp_mode,
                "tangent_mode": tangent_mode,
                "tangent_weight_mode": tangent_weight_mode,
                "time": r.read_f32()? as f64,
                "value": r.read_f32()? as f64,
                "arrive_tangent": r.read_f32()? as f64,
                "arrive_tangent_weight": r.read_f32()? as f64,
                "leave_tangent": r.read_f32()? as f64,
                "leave_tangent_weight": r.read_f32()? as f64,
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}
