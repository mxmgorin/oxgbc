use crate::video::gl_backend::GLSetup;
use gl::types::GLenum;
use serde::{Deserialize, Serialize};
use std::{ffi::CString, fmt};

const MASTER_VERTEX: &str = include_str!("../../../app/shaders/master.vert");
const PASSTHROUGH_FRAGMENT: &str = include_str!("../../../app/shaders/passthrough.frag");
const BILINEAR_FRAGMENT: &str = include_str!("../../../app/shaders/bilinear.frag");
const SMOOTH_BILINEAR_FRAGMENT: &str = include_str!("../../../app/shaders/smooth_bilinear.frag");
const CRT_FRAGMENT: &str = include_str!("../../../app/shaders/crt.frag");
const MASTER_FRAGMENT: &str = include_str!("../../../app/shaders/master.frag");
const FLAT_CRT_FRAGMENT: &str = include_str!("../../shaders/flat_crt.frag");
const HQ2X_FRAGMENT: &str = include_str!("../../shaders/hq2x.frag");
const AAOMNI_SCALE_LEGACY: &str = include_str!("../../shaders/aa_omni_scale_legacy.frag");
const AASCALE2X: &str = include_str!("../../shaders/aa_scale2x.frag");
const AASCALE4X: &str = include_str!("../../shaders/aa_scale4x.frag");
const LCD: &str = include_str!("../../shaders/lcd.frag");
const MONO_LCD: &str = include_str!("../../shaders/mono_lcd.frag");
const OMNI_SCALE: &str = include_str!("../../shaders/omni_scale.frag");
const SCALE2X: &str = include_str!("../../shaders/scale2x.frag");
const SCALE4X: &str = include_str!("../../shaders/scale4x.frag");

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[repr(u8)]
pub enum ShaderFrameBlendMode {
    None = 0,
    Simple = 1,
    AccEven = 2,
    AccOdd = 3,
}

impl fmt::Display for ShaderFrameBlendMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShaderFrameBlendMode::None => write!(f, "None"),
            ShaderFrameBlendMode::Simple => write!(f, "Simple"),
            ShaderFrameBlendMode::AccEven => write!(f, "Acc Even"),
            ShaderFrameBlendMode::AccOdd => write!(f, "Acc Odd"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[repr(u8)]
pub enum ShaderPrecision {
    Auto = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

pub const SHADERS: [(&str, &str); 14] = [
    ("Passthrough", PASSTHROUGH_FRAGMENT),
    ("Bilinear", BILINEAR_FRAGMENT),
    ("Smooth Bilinear", SMOOTH_BILINEAR_FRAGMENT),
    ("CRT", CRT_FRAGMENT),
    ("Flat CRT", FLAT_CRT_FRAGMENT),
    ("HQ2x", HQ2X_FRAGMENT),
    ("AA OmniScale Legacy", AAOMNI_SCALE_LEGACY),
    ("AA Scale2x", AASCALE2X),
    ("AA Scale4x", AASCALE4X),
    ("LCD", LCD),
    ("Mono LCD", MONO_LCD),
    ("OmniScale", OMNI_SCALE),
    ("Scale2x", SCALE2X),
    ("Scale4x", SCALE4X),
];

pub fn load_shader_program(
    name: &str,
    gl: &GLSetup,
    precision: ShaderPrecision,
) -> Result<u32, String> {
    for (i_name, shader) in SHADERS {
        if i_name.to_lowercase() == name.to_lowercase() {
            let frag_src = MASTER_FRAGMENT.replace("{filter}", shader);
            let frag = prepare_shader_source(gl, true, &frag_src, precision);
            let vert = prepare_shader_source(gl, false, MASTER_VERTEX, precision);

            log::debug!("Using fragment shader:\n{frag}");
            log::debug!("Using vertext shader:\n{vert}");

            return unsafe { compile_program(&vert, &frag) };
        }
    }

    Err(format!("Shader {name} not found"))
}

pub fn next_shader_by_name<'a>(current_name: &str) -> (&'a str, &'a str) {
    let idx = SHADERS
        .iter()
        .position(|(name, _)| *name == current_name)
        .unwrap_or(0);
    // Calculate next index with wrap-around
    let next_idx = (idx + 1) % SHADERS.len();

    SHADERS[next_idx]
}

pub fn prev_shader_by_name<'a>(current_name: &str) -> (&'a str, &'a str) {
    let idx = SHADERS
        .iter()
        .position(|(name, _)| *name == current_name)
        .unwrap_or(0);
    // Calculate previous index with wrap-around
    let prev_idx = if idx == 0 { SHADERS.len() - 1 } else { idx - 1 };

    SHADERS[prev_idx]
}

unsafe fn compile_program(vertex_src: &str, fragment_src: &str) -> Result<u32, String> {
    let vs = compile_shader(gl::VERTEX_SHADER, vertex_src)?;
    let fs = compile_shader(gl::FRAGMENT_SHADER, fragment_src)?;
    let program = gl::CreateProgram();
    gl::AttachShader(program, vs);
    gl::AttachShader(program, fs);
    gl::BindAttribLocation(program, 0, c"pos".as_ptr() as *const _);
    gl::BindAttribLocation(program, 1, c"tex".as_ptr() as *const _);
    gl::LinkProgram(program);
    gl::DeleteShader(vs);
    gl::DeleteShader(fs);
    let mut status = 0;
    gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);
    if status == 0 {
        let mut len = 0;
        gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; (len as usize) - 1];
        gl::GetProgramInfoLog(
            program,
            len,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
        );
        gl::DeleteProgram(program);
        return Err(String::from_utf8_lossy(&buf).into_owned());
    }

    Ok(program)
}

unsafe fn compile_shader(kind: GLenum, source: &str) -> Result<u32, String> {
    let shader = gl::CreateShader(kind);
    let c_str = CString::new(source).unwrap();
    gl::ShaderSource(shader, 1, &c_str.as_ptr(), std::ptr::null());
    gl::CompileShader(shader);
    let mut status = 0;
    gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
    if status == 0 {
        let mut len = 0;
        gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; (len as usize) - 1];
        gl::GetShaderInfoLog(
            shader,
            len,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
        );
        gl::DeleteShader(shader);
        return Err(String::from_utf8_lossy(&buf).into_owned());
    }
    Ok(shader)
}

/// Prepare shader with correct #version, per-stage precision and GLES2 rewrites.
/// `is_fragment` true for fragment shader, false for vertex shader.
/// `body` is the shader body (no #version or precision lines required).
pub fn prepare_shader_source(
    setup: &GLSetup,
    is_fragment: bool,
    body: &str,
    precision: ShaderPrecision,
) -> String {
    let mut header = String::new();
    header.push_str(setup.shader_version);
    header.push('\n');

    // For GLES we often need explicit precision qualifiers.
    // We'll emit per-stage precision using the detected best precision.
    if let Some(gles) = &setup.gles {
        let (frag_p, vert_p) = match precision {
            ShaderPrecision::Auto => (gles.fragment_precision, gles.vertex_precision),
            ShaderPrecision::Low => ("lowp", "lowp"),
            ShaderPrecision::Medium => ("mediump", "mediump"),
            ShaderPrecision::High => ("highp", "highp"),
        };

        if gles.is_version_2 {
            // GLES2: explicit precision qualifiers are required in fragment shaders,
            // and allowed in vertex shaders (though many implementations ignore them).
            if is_fragment {
                header.push_str(&format!("precision {frag_p} float;\n"));
                header.push_str(&format!("precision {frag_p} int;\n"));
            } else {
                // Vertex: emit precision too in case device needs it for varying math,
                // but many GLES2 vertex shaders can omit it (we include for safety).
                header.push_str(&format!("precision {vert_p} float;\n"));
                header.push_str(&format!("precision {vert_p} int;\n"));
            }
        } else {
            // GLES3: still useful to add fragment precision, but not strictly required.
            if is_fragment {
                header.push_str(&format!("precision {frag_p} float;\n"));
                header.push_str(&format!("precision {frag_p} int;\n"));
            }
        }
    }

    let mut processed = body.to_string();

    // If we landed on GLES2, perform automatic rewrites:
    if setup.gles.is_some() && setup.gles.as_ref().unwrap().is_version_2 {
        // 1) remove layout qualifiers (simple approach)
        //    This will remove occurrences like `layout(location = 0) ` (with or without spaces)
        processed = processed
            .replace("layout(location = ", "")
            .replace("layout(location=", "")
            // also remove trailing `)` from the above removals if any were left
            .replace(") ", " ")
            .replace(");", ";");

        // 2) convert `in`/`out` qualifier keywords to attribute/varying where appropriate.
        //    Naive text replacements suffice for typical simple shader bodies.
        if is_fragment {
            // In fragment: `in` -> `varying`, `out vec4 Name;` -> remove + replace uses with gl_FragColor
            processed = processed.replace("\nout ", "\n#OUT_MARKER "); // mark to handle types with 'out'
                                                                       // remove `out vec4 NAME;` (common case)
                                                                       // handle both `out vec4 FragColor;` and variants
            let mut lines: Vec<String> = Vec::new();
            for line in processed.lines() {
                if line.trim_start().starts_with("#OUT_MARKER") {
                    // skip this declaration line entirely
                    continue;
                } else {
                    lines.push(line.to_string());
                }
            }
            processed = lines.join("\n");
            processed = processed.replace("in ", "varying ");
            // replace remaining occurrences of the former frag-out name with gl_FragColor.
            // This is best-effort: look for common name `FragColor`
            processed = processed.replace("FragColor", "gl_FragColor");
        } else {
            // Vertex shader: `in` -> `attribute`, `out` -> `varying`
            processed = processed.replace("in ", "attribute ");
            processed = processed.replace("out ", "varying ");
        }

        // NOTE: This is a heuristic/textual transformation — works well for typical shaders.
        // For very complex shaders (structs, multiple declarations on one line, macros), consider using a small parser.
    }

    header.push_str(&processed);

    header
}
