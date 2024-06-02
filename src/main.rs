use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use eframe::egui::{self};

fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "DPI Modifier",
        options,
        Box::new(|_cc| Box::new(App::default())),
    );
}

#[derive(Default)]
struct App {
    folder_path: String,
    dpi: String,
    message: String,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DPI Modifier");

            ui.label("Folder Path:");
            ui.text_edit_singleline(&mut self.folder_path);

            ui.label("DPI:");
            ui.text_edit_singleline(&mut self.dpi);

            if ui.button("Run").clicked() {
                let dpi: u32 = match self.dpi.parse() {
                    Ok(dpi) => dpi,
                    Err(_) => {
                        self.message = "Invalid DPI value".to_string();
                        return;
                    }
                };

                if let Err(e) = process_folder(&self.folder_path, dpi) {
                    self.message = format!("Error: {:?}", e);
                } else {
                    self.message = "Processing complete".to_string();
                }
            }

            ui.add_space(10.0);
            ui.label(&self.message);
        });
    }
}

fn process_folder(folder_path: &str, new_dpi: u32) -> Result<(), Box<dyn std::error::Error>> {
    let original_folder = Path::new(folder_path);
    let modified_folder_path = original_folder.with_file_name("_modified");
    fs::create_dir_all(&modified_folder_path)?;

    for entry in WalkDir::new(folder_path) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(extension) = path.extension() {
                if extension == "png" {
                    println!("Processing file: {:?}", path);
                    if let Err(e) = modify_dpi(path, new_dpi, &modified_folder_path, original_folder) {
                        eprintln!("Error processing {:?}: {}", path, e);
                    }
                }
            }
        }
    }
    Ok(())
}

fn modify_dpi(path: &Path, dpi: u32, output_base: &Path, input_base: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ppm = (dpi as f32 * 39.3701) as u32;
    let mut phys_chunk_found = false;

    let input_file = File::open(path)?;
    let mut reader = BufReader::new(input_file);

    let relative_path = path.strip_prefix(input_base)?;
    let output_path = output_base.join(relative_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let output_file = OpenOptions::new().write(true).create(true).truncate(true).open(&output_path)?;
    let mut writer = BufWriter::new(output_file);

    let mut signature = [0; 8];
    reader.read_exact(&mut signature)?;
    writer.write_all(&signature)?;

    loop {
        let mut length_buf = [0; 4];
        if reader.read_exact(&mut length_buf).is_err() {
            break;
        }
        let length = u32::from_be_bytes(length_buf);

        let mut chunk_type = [0; 4];
        reader.read_exact(&mut chunk_type)?;

        let mut chunk_data = vec![0; length as usize];
        reader.read_exact(&mut chunk_data)?;

        let mut crc_buf = [0; 4];
        reader.read_exact(&mut crc_buf)?;

        if &chunk_type == b"pHYs" {
            phys_chunk_found = true;
            let x_ppm = u32::from_be_bytes([chunk_data[0], chunk_data[1], chunk_data[2], chunk_data[3]]);
            let y_ppm = u32::from_be_bytes([chunk_data[4], chunk_data[5], chunk_data[6], chunk_data[7]]);
            let x_dpi = x_ppm as f32 / 39.3701;
            let y_dpi = y_ppm as f32 / 39.3701;
            println!("original DPI: x = {:.2}, y = {:.2}", x_dpi, y_dpi);

            let mut phys_chunk = [0u8; 9];
            phys_chunk[0..4].copy_from_slice(&ppm.to_be_bytes());
            phys_chunk[4..8].copy_from_slice(&ppm.to_be_bytes());
            phys_chunk[8] = 1; // unit specifier: meters
            write_chunk(&mut writer, &chunk_type, &phys_chunk)?;
        } else {
            if &chunk_type == b"IDAT" && !phys_chunk_found {
                phys_chunk_found = true;
                let mut phys_chunk = [0u8; 9];
                phys_chunk[0..4].copy_from_slice(&ppm.to_be_bytes());
                phys_chunk[4..8].copy_from_slice(&ppm.to_be_bytes());
                phys_chunk[8] = 1; // unit specifier: meters
                write_chunk(&mut writer, b"pHYs", &phys_chunk)?;
            }
            write_chunk(&mut writer, &chunk_type, &chunk_data)?;
        }
    }

    if !phys_chunk_found {
        let mut phys_chunk = [0u8; 9];
        phys_chunk[0..4].copy_from_slice(&ppm.to_be_bytes());
        phys_chunk[4..8].copy_from_slice(&ppm.to_be_bytes());
        phys_chunk[8] = 1; // unit specifier: meters
        write_chunk(&mut writer, b"pHYs", &phys_chunk)?;
    }

    Ok(())
}

fn write_chunk<W: Write>(writer: &mut W, chunk_type: &[u8; 4], data: &[u8]) -> Result<(), std::io::Error> {
    let length = (data.len() as u32).to_be_bytes();
    writer.write_all(&length)?;
    writer.write_all(chunk_type)?;
    writer.write_all(data)?;
    let mut crc = crc32fast::Hasher::new();
    crc.update(chunk_type);
    crc.update(data);
    let crc = crc.finalize().to_be_bytes();
    writer.write_all(&crc)?;
    Ok(())
}
