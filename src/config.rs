use serde::{Serialize, Deserialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub period_ms: u32,
    pub rolling_x: bool,
    pub rolling_x_window: f64,
    pub min_x_bound: f64,
    pub min_y_bound: f64,
    pub max_x_bound: f64,
    pub max_y_bound: f64,
    pub window_width: f32,
    pub window_height: f32,
    pub display_hrm: bool,
    pub display_time: bool,
    pub display_plot: bool,
}

impl Default for Config{
    fn default() -> Self {
        Self {
            period_ms: 300,
            rolling_x: true,
            rolling_x_window: 60.0,
            min_x_bound: 0.0,
            min_y_bound: 0.0,
            max_x_bound: 10.0,
            max_y_bound: 200.0,
            window_width: 400.0,
            window_height: 300.0,
            display_hrm: true,
            display_time: true,
            display_plot: true,
        }
    }
}

impl Config {
    pub fn default_path() -> io::Result<PathBuf> {
        let mut config_path = std::env::current_exe()?;
        config_path.pop();
        config_path.push("heartrate_monitor_config.json");
        Ok(config_path)
    }

    pub fn load_config(config_path: &Path) -> io::Result<Self> {
        // create one if it doesn't exist
        if !config_path.exists() {
            let config_default = Config::default();
            std::fs::write(&config_path, serde_json::to_string_pretty(&config_default)?)?;
            println!("Could not find existing config JSON, created default at: {:?}", config_path);
            return Ok(config_default);
        }
        let config: Config = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
        Ok(config)
    }

    pub fn save_config(&self, config_path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(config_path, json)?;
        println!("Saved config JSON file to: {:?}", config_path);
        Ok(())

    }
}