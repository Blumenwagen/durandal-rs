#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub docker: DockerConfig,
}

impl Config {
    pub fn from_text(text: &str) -> anyhow::Result<Self> {
        let mut config = Self::default();
        let mut section = String::new();

        for raw_line in text.lines() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                anyhow::bail!("invalid config line: {raw_line}");
            };
            let key = key.trim().to_ascii_lowercase();
            let value = unquote(value.trim());
            match (section.as_str(), key.as_str()) {
                ("docker", "controls") => {
                    config.docker.controls = DockerControlsMode::parse(value)?;
                }
                ("docker", "show_panel") => {
                    config.docker.show_panel_by_default = parse_bool(value)?;
                }
                _ => {}
            }
        }

        Ok(config)
    }

    pub fn docker_controls_enabled(&self) -> bool {
        self.docker.controls_enabled()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerConfig {
    pub controls: DockerControlsMode,
    pub show_panel_by_default: bool,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            controls: DockerControlsMode::Enabled,
            show_panel_by_default: true,
        }
    }
}

impl DockerConfig {
    pub fn controls_enabled(&self) -> bool {
        self.controls == DockerControlsMode::Enabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerControlsMode {
    Hidden,
    ReadOnly,
    Enabled,
}

impl DockerControlsMode {
    fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hidden" | "off" | "false" | "disabled" => Ok(Self::Hidden),
            "readonly" | "read_only" | "read-only" | "visible" => Ok(Self::ReadOnly),
            "enabled" | "on" | "true" => Ok(Self::Enabled),
            other => anyhow::bail!("invalid docker controls mode {other:?}"),
        }
    }
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        other => anyhow::bail!("invalid boolean {other:?}"),
    }
}
