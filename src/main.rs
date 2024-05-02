#![feature(lazy_cell)]

use std::sync::{LazyLock, Mutex};

use anyhow::Context;
use eframe::{egui, Frame};
use egui::{Layout, RichText};
use itertools::Itertools;
use windows::{
    core::{GUID, HRESULT, PCSTR},
    Win32::{
        System::{
            Com::{CoCreateInstance, CLSCTX_ALL},
            SystemServices::SFGAO_FOLDER,
        },
        UI::{
            Shell::{
                FileOpenDialog, IFileDialog, IFileDialog2, IFileOpenDialog, IShellDispatch,
                SHCoCreateInstance, ShellBrowserWindow, ShellDispatchInproc, FOS_PICKFOLDERS,
                SHGFI_ATTRIBUTES, SIGDN, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{DialogBoxParamA, MessageBoxA, MB_OK},
        },
    },
};

use windows::core::s;

static SPT_FOLDER: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

static CLIENT_MODS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static SERVER_MODS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "SPT Mod Organizer",
        native_options,
        Box::new(|cc| Box::new(App::new(cc))),
    )
}

#[derive(Default)]
struct App {}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Info)
            .init()
            .unwrap();

        std::thread::spawn(|| loop {
            match || -> anyhow::Result<()> {
                if let Some(spt_folder) = SPT_FOLDER.lock().unwrap().as_ref() {
                    let path = std::path::Path::new(spt_folder);

                    const CLIENT_MODS_FOLDER: &str = "BepInEx/plugins";
                    const SERVER_MODS_FOLDER: &str = "user/mods";

                    let client_mods_folder = path.join(CLIENT_MODS_FOLDER);
                    let server_mods_folder = path.join(SERVER_MODS_FOLDER);

                    if client_mods_folder.exists() {
                        let mut client_mods: Vec<String> = client_mods_folder
                            .read_dir()?
                            .filter_map(|entry| {
                                entry.ok().and_then(|entry| {
                                    if entry.file_type().ok()?.is_dir() {
                                        if ["spt", "ssh"].contains(
                                            &entry
                                                .file_name()
                                                .to_string_lossy()
                                                .to_string()
                                                .as_str(),
                                        ) {
                                            return None;
                                        }

                                        Some(entry.file_name().to_string_lossy().to_string())
                                    } else if entry.file_type().ok()?.is_file() {
                                        if entry.file_name().to_string_lossy().ends_with(".dll") {
                                            Some(
                                                entry
                                                    .file_name()
                                                    .to_string_lossy()
                                                    .to_string()
                                                    .replace(".dll", ""),
                                            )
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                })
                            })
                            .collect();

                        client_mods.dedup();

                        *CLIENT_MODS.lock().unwrap() = client_mods;
                    }

                    if server_mods_folder.exists() {
                        let mut server_mods: Vec<String> = server_mods_folder
                            .read_dir()?
                            .filter_map(|entry| {
                                entry.ok().and_then(|entry| {
                                    if entry.file_type().ok()?.is_dir() {
                                        let package_json = entry.path().join("package.json");

                                        match package_json.exists() {
                                            true => Some(
                                                entry.file_name().to_string_lossy().to_string(),
                                            ),
                                            false => {
                                                log::warn!(
                                                    "Server mod {} does not have a package.json",
                                                    entry.file_name().to_string_lossy()
                                                );

                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                })
                            })
                            .collect();

                        server_mods.dedup();

                        *SERVER_MODS.lock().unwrap() = server_mods;
                    }
                }

                Ok(())
            }() {
                Ok(()) => {}
                Err(error) => {
                    log::error!("{:#}", error)
                }
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        });

        Self::default()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("TopBar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| if ui.button("Import Mod...").clicked() {});

                ui.menu_button("Settings", |ui| {
                    if ui.button("Set SPT Path").clicked() {
                        unsafe {
                            match || -> anyhow::Result<String> {
                                let windows: IFileDialog =
                                    CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)
                                        .context("Failed to create file dialog instance")?;

                                windows
                                    .SetOptions(FOS_PICKFOLDERS)
                                    .context("Failed to set options")?;

                                windows.Show(None).context("Failed to show file dialog")?;

                                let folder = windows.GetFolder()?;

                                let path = folder.GetDisplayName(SIGDN_FILESYSPATH)?.to_string()?;

                                println!("{:?}", path);

                                Ok(path)
                            }() {
                                Ok(file_name) => {
                                    *SPT_FOLDER.lock().unwrap() = Some(file_name);
                                }
                                Err(error) => {
                                    MessageBoxA(
                                        None,
                                        PCSTR::from_raw(format!("{:#}", error).as_ptr()),
                                        s!("Error"),
                                        MB_OK,
                                    );
                                }
                            }
                        }
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(spt_folder) = SPT_FOLDER.lock().unwrap().as_ref() {
                ui.label(format!("SPT Folder: {}", spt_folder));

                ui.separator();

                ui.horizontal(|ui| {
                    egui::Frame::canvas(ui.style()).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.heading("Client Mods");

                            let client_mods = CLIENT_MODS.lock().unwrap();

                            for client_mod in client_mods.iter() {
                                ui.label(client_mod);
                            }
                        });
                    });

                    egui::Frame::canvas(ui.style()).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.heading("Server Mods");

                            let server_mods = SERVER_MODS.lock().unwrap();

                            for server_mod in server_mods.iter() {
                                ui.label(server_mod);
                            }
                        });
                    });
                });
            } else {
                ui.label("Please set the SPT folder");
            }
        });
    }
}
