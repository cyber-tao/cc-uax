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
        // Explicit-precision variants (NoExportTypes.h, `immutable` in 5.0–5.8).
        "Vector3d" => vec3(r, true)?,
        "Vector4d" | "Quat4d" => vec4(r, true)?,
        "Rotator3d" => json!({
            "pitch": r.read_f64()?, "yaw": r.read_f64()?, "roll": r.read_f64()?
        }),
        // FPlane : FVector + W, so the whole thing follows the LWC gate.
        "Plane" => vec4(r, lwc)?,
        "Plane4f" => vec4(r, false)?,
        "Plane4d" => vec4(r, true)?,
        "TwoVectors" => json!({ "v1": vec3(r, lwc)?, "v2": vec3(r, lwc)? }),
        "Ray" => json!({ "origin": vec3(r, lwc)?, "direction": vec3(r, lwc)? }),
        "Ray3f" => json!({ "origin": vec3(r, false)?, "direction": vec3(r, false)? }),
        "Ray3d" => json!({ "origin": vec3(r, true)?, "direction": vec3(r, true)? }),
        "Sphere" => json!({ "center": vec3(r, lwc)?, "radius": read_coord(r, lwc)? }),
        "Sphere3f" => json!({ "center": vec3(r, false)?, "radius": r.read_f32()? as f64 }),
        "Sphere3d" => json!({ "center": vec3(r, true)?, "radius": r.read_f64()? }),
        "OrientedBox" => json!({
            "center": vec3(r, lwc)?,
            "axis_x": vec3(r, lwc)?,
            "axis_y": vec3(r, lwc)?,
            "axis_z": vec3(r, lwc)?,
            "extent_x": read_coord(r, lwc)?,
            "extent_y": read_coord(r, lwc)?,
            "extent_z": read_coord(r, lwc)?,
        }),
        "Box3d" => {
            let min = vec3(r, true)?;
            let max = vec3(r, true)?;
            json!({ "min": min, "max": max, "is_valid": r.read_u8()? != 0 })
        }
        "Matrix44d" => {
            let mut m = Vec::with_capacity(16);
            for _ in 0..16 {
                m.push(json!(r.read_f64()?));
            }
            json!({ "m": m })
        }
        "PackedNormal" => json!({
            "x": r.read_u8()?, "y": r.read_u8()?, "z": r.read_u8()?, "w": r.read_u8()?
        }),
        "PackedRGB10A2N" => json!({ "packed": r.read_i32()? }),
        "PackedRGBA16N" => json!({ "xy": r.read_i32()?, "zw": r.read_i32()? }),
        // Integer points, rects and vectors of every width (5.8 added the
        // explicit-width names; the layout is the obvious sequence of components).
        "IntPoint" | "Int32Point" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "Int64Point" => json!({ "x": r.read_i64()?, "y": r.read_i64()? }),
        "Uint32Point" => json!({ "x": r.read_u32()?, "y": r.read_u32()? }),
        "Uint64Point" => json!({ "x": r.read_u64()?, "y": r.read_u64()? }),
        "IntRect" | "Int32Rect" => json!({
            "min": { "x": r.read_i32()?, "y": r.read_i32()? },
            "max": { "x": r.read_i32()?, "y": r.read_i32()? }
        }),
        "Int64Rect" => json!({
            "min": { "x": r.read_i64()?, "y": r.read_i64()? },
            "max": { "x": r.read_i64()?, "y": r.read_i64()? }
        }),
        "UintRect" | "Uint32Rect" => json!({
            "min": { "x": r.read_u32()?, "y": r.read_u32()? },
            "max": { "x": r.read_u32()?, "y": r.read_u32()? }
        }),
        "Uint64Rect" => json!({
            "min": { "x": r.read_u64()?, "y": r.read_u64()? },
            "max": { "x": r.read_u64()?, "y": r.read_u64()? }
        }),
        "IntVector" | "Int32Vector" => {
            json!({ "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()? })
        }
        "Int64Vector" => json!({ "x": r.read_i64()?, "y": r.read_i64()?, "z": r.read_i64()? }),
        "Uint32Vector" => json!({ "x": r.read_u32()?, "y": r.read_u32()?, "z": r.read_u32()? }),
        "Uint64Vector" => json!({ "x": r.read_u64()?, "y": r.read_u64()?, "z": r.read_u64()? }),
        "Int64Vector2" => json!({ "x": r.read_i64()?, "y": r.read_i64()? }),
        "Uint32Vector2" => json!({ "x": r.read_u32()?, "y": r.read_u32()? }),
        "Uint64Vector2" => json!({ "x": r.read_u64()?, "y": r.read_u64()? }),
        "Int64Vector4" => json!({
            "x": r.read_i64()?, "y": r.read_i64()?, "z": r.read_i64()?, "w": r.read_i64()?
        }),
        "UintVector4" | "Uint32Vector4" => json!({
            "x": r.read_u32()?, "y": r.read_u32()?, "z": r.read_u32()?, "w": r.read_u32()?
        }),
        "Uint64Vector4" => json!({
            "x": r.read_u64()?, "y": r.read_u64()?, "z": r.read_u64()?, "w": r.read_u64()?
        }),
        "Guid" => json!(r.read_guid()?.to_hex()),
        "Color" => json!({
            "b": r.read_u8()?, "g": r.read_u8()?, "r": r.read_u8()?, "a": r.read_u8()?
        }),
        "LinearColor" => json!({
            "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
        }),
        "DateTime" | "Timespan" => json!(r.read_i64()?),
        // `Transform` deliberately has no arm. Alone among the core math types,
        // FTransform's USTRUCT is *not* `immutable` (NoExportTypes.h, UE5.0-5.8)
        // and TTransformStructOpsTypeTraits keeps `WithSerializer` commented out,
        // so UScriptStruct::SerializeItem falls through to
        // SerializeTaggedProperties: a StructProperty(Transform) payload is a
        // tagged Rotation/Translation/Scale3D block, not three packed vectors.
        // The explicit `FTransform3f`/`FTransform3d` variants *are* immutable and
        // so keep their binary layout.
        "Transform3f" => {
            let rot = json!({
                "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
            });
            let trans = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            let scale = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
        }
        "Transform3d" => {
            json!({
                "rotation": json!({
                    "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
                }),
                "translation": json!({
                    "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?
                }),
                "scale3d": json!({
                    "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?
                })
            })
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
        "IntVector2" | "Int32Vector2" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector4" | "Int32Vector4" => json!({
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
