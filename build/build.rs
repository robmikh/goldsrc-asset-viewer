extern crate shaderc;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let base_shader_path = "data/shaders";
    let generated_shader_path = "data/generated/shaders";

    println!("cargo:rerun-if-changed={}", base_shader_path);
    for entry in std::fs::read_dir(base_shader_path)? {
        let entry = entry?;

        println!(
            "cargo:rerun-if-changed={}/{}",
            base_shader_path,
            entry.path().file_name().unwrap().to_string_lossy()
        );
    }

    std::fs::create_dir_all(generated_shader_path)?;

    let mut compiler = shaderc::Compiler::new().unwrap();
    let mut options = shaderc::CompileOptions::new().unwrap();
    options.add_macro_definition("EP", Some("main"));

    for entry in std::fs::read_dir(base_shader_path)? {
        let entry = entry?;

        if entry.file_type()?.is_file() {
            let in_path = entry.path();

            let shader_type =
                in_path
                    .extension()
                    .and_then(|ext| match ext.to_string_lossy().as_ref() {
                        "vert" => Some(shaderc::ShaderKind::Vertex),
                        "frag" => Some(shaderc::ShaderKind::Fragment),
                        _ => None,
                    });

            if let Some(shader_type) = shader_type {
                let source = std::fs::read_to_string(&in_path)?;
                let compiled_file = compiler
                    .compile_into_spirv(
                        &source,
                        shader_type,
                        &in_path.file_name().unwrap().to_string_lossy(),
                        "main",
                        Some(&options),
                    )
                    .unwrap();

                let compiled_bytes = compiled_file.as_binary_u8();

                let out_path = format!(
                    "{}/{}.spv",
                    generated_shader_path,
                    in_path.file_name().unwrap().to_string_lossy()
                );

                std::fs::write(&out_path, &compiled_bytes)?;
            }
        }
    }

    Ok(())
}
