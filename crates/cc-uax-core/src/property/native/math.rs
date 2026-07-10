use crate::reader::Reader;
use crate::structured_value::{Value, json};
use anyhow::Result;
// structs). None of these need name resolution or the value window.
pub(super) fn parse_math_struct(r: &mut Reader, name: &str) -> Result<Option<Value>> {
    let v = match name {
        // Note: FVector_NetQuantize* subclasses only declare WithNetSerializer, so
        // their package payload is tagged properties — do not decode them natively.
        "Vector" => {
            json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? })
        }
        "Vector3f" => json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? }),
        "Vector2D" => json!({ "x": r.read_f64()?, "y": r.read_f64()? }),
        "Vector2f" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "Vector4" => json!({
            "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
        }),
        "Vector4f" => json!({
            "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
        }),
        "Rotator" => json!({
            "pitch": r.read_f64()?, "yaw": r.read_f64()?, "roll": r.read_f64()?
        }),
        "Quat" => json!({
            "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
        }),
        "IntPoint" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
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
            let rot = json!({
                "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
            });
            let trans = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let scale = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
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
            let min = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let max = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "Box2D" => {
            let min = json!({ "x": r.read_f64()?, "y": r.read_f64()? });
            let max = json!({ "x": r.read_f64()?, "y": r.read_f64()? });
            // TBox2::Serialize writes bIsValid as a single uint8 (not a 4-byte UBOOL).
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "Box2f" => {
            let min = json!({ "x": r.read_f32()?, "y": r.read_f32()? });
            let max = json!({ "x": r.read_f32()?, "y": r.read_f32()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "FrameNumber" => json!({ "value": r.read_i32()? }),
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
