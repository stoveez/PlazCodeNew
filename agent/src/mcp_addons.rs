use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub servers: BTreeMap<String, ServerSpec>,
}

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerSummary {
    pub id: String,
    pub name: String,
    pub alive: bool,
    pub tools: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn catalog() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            id: "blender",
            name: "Blender",
            description: "3D scene, mesh and asset tools through Blender MCP.",
            command: "uvx",
            args: &["blender-mcp"],
        },
        CatalogEntry {
            id: "context7",
            name: "Context7",
            description: "Library and framework documentation lookup.",
            command: "npx",
            args: &["-y", "@upstash/context7-mcp"],
        },
        CatalogEntry {
            id: "fetch",
            name: "Fetch",
            description: "Fetch and read web resources through an MCP server.",
            command: "uvx",
            args: &["mcp-server-fetch"],
        },
        CatalogEntry {
            id: "git",
            name: "Git",
            description: "Local Git repository inspection and operations.",
            command: "uvx",
            args: &["mcp-server-git"],
        },
        CatalogEntry {
            id: "memory",
            name: "Memory",
            description: "Local graph-style memory tools for agent sessions.",
            command: "npx",
            args: &["-y", "@modelcontextprotocol/server-memory"],
        },
        CatalogEntry {
            id: "thinking",
            name: "Sequential Thinking",
            description: "Structured step-by-step reasoning utilities.",
            command: "npx",
            args: &["-y", "@modelcontextprotocol/server-sequential-thinking"],
        },
    ]
}

pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.json")))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

pub fn read_config() -> McpConfig {
    let path = config_path();
    let mut cfg = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<McpConfig>(&raw).ok())
        .unwrap_or_default();
    cfg.servers.entry("roblox".to_string()).or_insert_with(|| ServerSpec {
        command: "launch_studio_mcp.py".to_string(),
        args: Vec::new(),
        env: HashMap::new(),
    });
    cfg
}

pub fn write_config(cfg: &McpConfig) -> anyhow::Result<()> {
    let path = config_path();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(cfg)?)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn is_enabled(id: &str) -> bool {
    read_config().servers.contains_key(id)
}

pub fn set_catalog_enabled(id: &str, enabled: bool) -> anyhow::Result<()> {
    let entry = catalog().into_iter().find(|e| e.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown catalog server '{id}'"))?;
    let mut cfg = read_config();
    if enabled {
        cfg.servers.insert(id.to_string(), ServerSpec {
            command: entry.command.to_string(),
            args: entry.args.iter().map(|v| (*v).to_string()).collect(),
            env: HashMap::new(),
        });
    } else {
        cfg.servers.remove(id);
    }
    write_config(&cfg)
}

pub fn add_server(id: &str, spec: ServerSpec) -> anyhow::Result<()> {
    let id = id.trim();
    anyhow::ensure!(!id.is_empty(), "server id is required");
    anyhow::ensure!(id != "roblox", "the Roblox server cannot be replaced");
    anyhow::ensure!(!spec.command.trim().is_empty(), "server command is required");
    let mut cfg = read_config();
    cfg.servers.insert(id.to_string(), spec);
    write_config(&cfg)
}

pub fn remove_server(id: &str) -> anyhow::Result<()> {
    let id = id.trim();
    anyhow::ensure!(id != "roblox", "the Roblox server cannot be removed");
    let mut cfg = read_config();
    anyhow::ensure!(cfg.servers.remove(id).is_some(), "server '{id}' is not configured");
    write_config(&cfg)
}

struct AddonRuntime {
    id: String,
    spec: ServerSpec,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<Lines<BufReader<ChildStdout>>>,
    next_id: u64,
    tools: Vec<serde_json::Value>,
    last_error: Option<String>,
}

impl AddonRuntime {
    fn new(id: String, spec: ServerSpec) -> Self {
        Self {
            id,
            spec,
            child: None,
            stdin: None,
            stdout: None,
            next_id: 1,
            tools: Vec::new(),
            last_error: None,
        }
    }

    fn same_spec(&self, spec: &ServerSpec) -> bool {
        &self.spec == spec
    }

    fn child_alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    async fn reset(&mut self) {
        if let Some(child) = self.child.as_mut() {
            #[cfg(windows)]
            if let Some(pid) = child.id() {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output();
            }
            let _ = child.kill().await;
        }
        self.child = None;
        self.stdin = None;
        self.stdout = None;
        self.tools.clear();
        self.next_id = 1;
    }

    fn command(&self) -> Command {
        #[cfg(windows)]
        {
            let low = self.spec.command.to_ascii_lowercase();
            if low == "npx" || low.ends_with(".cmd") || low.ends_with(".bat") {
                let mut cmd = Command::new("cmd");
                cmd.arg("/C").arg(&self.spec.command).args(&self.spec.args);
                cmd.envs(&self.spec.env);
                cmd.creation_flags(CREATE_NO_WINDOW);
                return cmd;
            }
        }
        let mut cmd = Command::new(&self.spec.command);
        cmd.args(&self.spec.args).envs(&self.spec.env);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    async fn ensure(&mut self) -> anyhow::Result<()> {
        if self.child_alive() && self.stdin.is_some() && self.stdout.is_some() {
            return Ok(());
        }
        self.reset().await;
        let mut cmd = self.command();
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("[{}] could not start '{}': {e}", self.id, self.spec.command))?;
        self.stdin = child.stdin.take();
        self.stdout = child.stdout.take().map(|s| BufReader::new(s).lines());
        self.child = Some(child);
        self.request("initialize", serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "PlazCode", "version": env!("CARGO_PKG_VERSION")}
        })).await?;
        self.notify("notifications/initialized", serde_json::json!({})).await?;
        self.last_error = None;
        Ok(())
    }

    async fn notify(&mut self, method: &str, params: serde_json::Value) -> anyhow::Result<()> {
        let line = serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}).to_string() + "\n";
        self.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("MCP stdin unavailable"))?.write_all(line.as_bytes()).await?;
        self.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("MCP stdin unavailable"))?.flush().await?;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let request_id = self.next_id;
        self.next_id += 1;
        let line = serde_json::json!({"jsonrpc":"2.0","id":request_id,"method":method,"params":params}).to_string() + "\n";
        let stdin = self.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("MCP stdin unavailable"))?;
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        let stdout = self.stdout.as_mut().ok_or_else(|| anyhow::anyhow!("MCP stdout unavailable"))?;
        loop {
            let line = tokio::time::timeout(std::time::Duration::from_secs(120), stdout.next_line()).await
                .map_err(|_| anyhow::anyhow!("[{}] MCP request timed out: {method}", self.id))??
                .ok_or_else(|| anyhow::anyhow!("[{}] MCP server exited while handling {method}", self.id))?;
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else { continue; };
            if message.get("id").and_then(|v| v.as_u64()) != Some(request_id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                anyhow::bail!("[{}] {method} failed: {error}", self.id);
            }
            return message.get("result").cloned().ok_or_else(|| anyhow::anyhow!("[{}] {method} returned no result", self.id));
        }
    }

    async fn list_tools(&mut self) -> anyhow::Result<Vec<serde_json::Value>> {
        self.ensure().await?;
        let result = self.request("tools/list", serde_json::json!({})).await?;
        self.tools = result.get("tools").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(self.tools.clone())
    }

    async fn call_tool(&mut self, name: &str, args: serde_json::Value) -> anyhow::Result<(String, Vec<serde_json::Value>)> {
        self.ensure().await?;
        let result = self.request("tools/call", serde_json::json!({"name":name,"arguments":args})).await?;
        let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut texts = Vec::new();
        let mut images = Vec::new();
        if let Some(items) = result.get("content").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    texts.push(text.to_string());
                }
                if item.get("type").and_then(|v| v.as_str()) == Some("image") {
                    if let Some(data) = item.get("data").and_then(|v| v.as_str()) {
                        images.push(serde_json::json!({
                            "mimeType": item.get("mimeType").and_then(|v| v.as_str()).unwrap_or("image/png"),
                            "data": data
                        }));
                    }
                }
            }
        }
        let text = texts.join("\n");
        if is_error {
            anyhow::bail!("{}", if text.is_empty() { result.to_string() } else { text });
        }
        Ok((text, images))
    }
}

pub struct AddonManager {
    runtimes: HashMap<String, AddonRuntime>,
}

impl AddonManager {
    pub fn new() -> Self {
        Self { runtimes: HashMap::new() }
    }

    async fn sync_config(&mut self) {
        let cfg = read_config();
        let wanted: HashMap<String, ServerSpec> = cfg.servers.into_iter()
            .filter(|(id, _)| id != "roblox")
            .collect();

        let stale: Vec<String> = self.runtimes.keys()
            .filter(|id| !wanted.contains_key(*id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(mut runtime) = self.runtimes.remove(&id) {
                runtime.reset().await;
            }
        }

        for (id, spec) in wanted {
            let replace = self.runtimes.get(&id).map(|r| !r.same_spec(&spec)).unwrap_or(true);
            if replace {
                if let Some(mut old) = self.runtimes.remove(&id) {
                    old.reset().await;
                }
                self.runtimes.insert(id.clone(), AddonRuntime::new(id, spec));
            }
        }
    }

    pub async fn reset_all(&mut self) {
        for runtime in self.runtimes.values_mut() {
            runtime.reset().await;
        }
        self.runtimes.clear();
    }

    pub async fn list_tools(&mut self) -> (Vec<serde_json::Value>, Vec<ServerSummary>) {
        self.sync_config().await;
        let mut tools = Vec::new();
        let mut summaries = Vec::new();
        let ids: Vec<String> = self.runtimes.keys().cloned().collect();
        for id in ids {
            let Some(runtime) = self.runtimes.get_mut(&id) else { continue; };
            match runtime.list_tools().await {
                Ok(server_tools) => {
                    for mut tool in server_tools {
                        let Some(real) = tool.get("name").and_then(|v| v.as_str()).map(str::to_string) else { continue; };
                        tool["name"] = serde_json::Value::String(format!("{id}__{real}"));
                        if let Some(desc) = tool.get("description").and_then(|v| v.as_str()).map(str::to_string) {
                            tool["description"] = serde_json::Value::String(format!("[{id}] {desc}"));
                        }
                        tool["server"] = serde_json::Value::String(id.clone());
                        tools.push(tool);
                    }
                    summaries.push(ServerSummary {
                        id: id.clone(),
                        name: id.clone(),
                        alive: true,
                        tools: runtime.tools.len(),
                        error: None,
                    });
                }
                Err(error) => {
                    runtime.last_error = Some(error.to_string());
                    summaries.push(ServerSummary {
                        id: id.clone(),
                        name: id.clone(),
                        alive: false,
                        tools: 0,
                        error: runtime.last_error.clone(),
                    });
                }
            }
        }
        summaries.sort_by(|a, b| a.id.cmp(&b.id));
        (tools, summaries)
    }

    pub async fn call_tool(&mut self, advertised: &str, args: serde_json::Value) -> anyhow::Result<(String, Vec<serde_json::Value>)> {
        self.sync_config().await;
        let (id, real) = advertised.split_once("__")
            .ok_or_else(|| anyhow::anyhow!("invalid add-on tool name '{advertised}'"))?;
        let runtime = self.runtimes.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{id}' is not configured"))?;
        runtime.call_tool(real, args).await
    }
}
