use std::{
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufReader, Write},
    path::Path,
};

use autd3_simulator::{Simulator, State};

fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find(',')
        .ok_or_else(|| format!("no `,` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

struct Args {
    window_size: Option<(u32, u32)>,
    port: Option<u16>,
    vsync: Option<bool>,
    setting_dir: Option<String>,
    setting_file: String,
    debug: bool,
}

impl Args {
    fn parse() -> Result<Self, Box<dyn Error>> {
        let mut args = env::args().skip(1);
        let mut window_size = None;
        let mut port = None;
        let mut vsync = None;
        let mut setting_dir = None;
        let mut setting_file = String::from("settings.json");
        let mut debug = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-w" | "--window_size" => {
                    let val = args
                        .next()
                        .ok_or("--window_size requires a value (Width,Height)")?;
                    window_size = Some(parse_key_val(&val).map_err(|e| e.to_string())?);
                }
                "-p" | "--port" => {
                    let val = args.next().ok_or("--port requires a value")?;
                    port = Some(
                        val.parse()
                            .map_err(|e: std::num::ParseIntError| e.to_string())?,
                    );
                }
                "-v" | "--vsync" => {
                    let val = args.next().ok_or("--vsync requires a value")?;
                    vsync = Some(
                        val.parse()
                            .map_err(|e: std::str::ParseBoolError| e.to_string())?,
                    );
                }
                "--setting_dir" => {
                    setting_dir = Some(args.next().ok_or("--setting_dir requires a value")?);
                }
                "-s" | "--setting_file" => {
                    setting_file = args.next().ok_or("--setting_file requires a value")?;
                }
                "-d" | "--debug" => {
                    debug = true;
                }
                "-h" | "--help" => {
                    Self::print_help();
                    std::process::exit(0);
                }
                "--version" => {
                    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                _ => {
                    return Err(format!("Unknown argument: {}", arg).into());
                }
            }
        }

        Ok(Self {
            window_size,
            port,
            vsync,
            setting_dir,
            setting_file,
            debug,
        })
    }

    fn print_help() {
        println!("Author: {} ", env!("CARGO_PKG_AUTHORS"));
        println!("{}", env!("CARGO_PKG_DESCRIPTION"));
        println!("Version: {} \n", env!("CARGO_PKG_VERSION"));
        println!("USAGE:");
        println!("    {} [OPTIONS]\n", env!("CARGO_PKG_NAME"));
        println!("OPTIONS:");
        println!("    -w, --window_size <Width,Height>");
        println!("            Windows Size (Optional, if set, overrides settings from file)\n");
        println!("    -p, --port <PORT>");
        println!("            Port (Optional, if set, overrides settings from file)\n");
        println!("    -v, --vsync <VSYNC>");
        println!("            Vsync (Optional, if set, overrides settings from file)\n");
        println!("    --setting_dir <DIR>");
        println!("            Setting file dir\n");
        println!("    -s, --setting_file <FILE>");
        println!("            Setting file name [default: settings.json]\n");
        println!("    -d, --debug");
        println!("            Debug mode\n");
        println!("    -h, --help");
        println!("            Print help\n");
        println!("    --version");
        println!("            Print version");
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arg = Args::parse()?;

    let port = arg.port;
    let window_size = arg.window_size;
    let settings_path = if let Some(path) = &arg.setting_dir {
        Path::new(path).join(&arg.setting_file)
    } else {
        Path::new(&arg.setting_file).to_owned()
    };
    let vsync = arg.vsync;
    let debug = arg.debug;

    let mut state: State = if settings_path.exists() {
        let file = File::open(&settings_path)?;
        let reader = BufReader::new(file);
        match serde_json::from_reader(reader) {
            Ok(state) => state,
            Err(e) => {
                eprintln!(
                    "Failed to parse settings file ({}): {}, using default settings.",
                    settings_path.display(),
                    e
                );
                Default::default()
            }
        }
    } else {
        eprintln!(
            "Settings file ({}) not found, using default settings.",
            settings_path.display()
        );
        Default::default()
    };

    state.debug = debug;
    if let Some(port) = port {
        state.port = port;
    }
    if let Some(window_size) = window_size {
        state.window_size = window_size;
    }
    if let Some(vsync) = vsync {
        state.vsync = vsync;
    }
    if let Some(path) = &arg.setting_dir {
        state.settings_dir = path.clone();
    }

    let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    let state = Simulator::run(event_loop, state)?;

    {
        let settings_str = serde_json::to_string_pretty(&state)?;
        if settings_path.exists() {
            fs::remove_file(&settings_path)?;
        }
        std::fs::create_dir_all(settings_path.parent().unwrap())?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .append(false)
            .open(&settings_path)?;
        write!(file, "{settings_str}")?;
    }

    Ok(())
}
