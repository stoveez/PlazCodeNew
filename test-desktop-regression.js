const fs = require("fs");
const crypto = require("crypto");

const baseline = {
  "core/agent_skills.js": "86e63d2ecefeafd586744411650cb446c0db2a5f",
  "core/animlib.js": "420ea8eb69637b27ca9856b974d44181e57d3420",
  "core/config.js": "51a52f333caab9755fcbdb31809f66a891e5e77a",
  "core/headless-builder.js": "b343a897a17fc05d79ab469aeea55c8abaa8c4e5",
  "core/luau-knowledge.js": "234c1ca2155d3ef5f38927d85893b70fd6bde6e6",
  "core/main.js": "61cb220aabf19d249876d14b57c1420147c354e3",
  "core/motion-interchange.js": "038afa13a5b59c030a809abf7d617a36f2fae82d",
  "core/motion-preview.js": "ae0ab0526fbc762dfbf7cae9c248239e217e7a2f",
  "core/motion-tools.js": "8ac81b4a4a9f634fef9285aeee06db4a15af578e",
  "core/parser.js": "8b428530f3d861bd73dc9da0be5d52dba1fa37b8",
  "core/studio_daily.js": "56632a6bd1a81ddeded243ff9d740e4b75a4fa34",
  "core/studio_gui.js": "726793b63c24b3fd128be60e6f3fb98e60a19243",
  "core/studio_plus.js": "c149c47cc7d178e3aa87f39539cc3c1420bff065",
  "core/studio_skills.js": "3f549ef8b67a4a646b0a3d3219ecf38b0839e09e",
  "providers/arena.js": "8e7bf69502fe37948ba11940763752454c72da16",
  "providers/chatgpt-cm.js": "864c7864a46908b76b2da835f30c946d66e9182b",
  "providers/chatgpt.js": "0bcfa90ffa431ad14601c01f4bc6d0b698db0d3e",
  "providers/claude.js": "d1929e76a1d586a066568091699ee7117709cb38",
  "providers/copilot.js": "e36624c3c017f5cbeb8f2e1317097a56551aa967",
  "providers/crax-net.js": "a3c1bfb62a62594a2d6eef3615f66d4aa53b9157",
  "providers/crax.js": "2b9b0af5f5b615c3ab4912c6a3f535586a0d385e",
  "providers/deepseek.js": "6361861471d1acba65b928af7ccd1cb3d6cd8522",
  "providers/freebuff.js": "f058d5b5f449a59582e58fc45064299cc5287b4f",
  "providers/gemini.js": "c2e7bf002de9fb3492075677a085fb4382bf74a1",
  "providers/glm.js": "4dc3684dbca0edfe4bb3d904660ee2b20f1bdb31",
  "providers/kimi.js": "1b7bba57787adc548eb5fc42b3d40cb13b2b9057",
  "providers/meta.js": "8a73d83b93dc2b20c0374dcc7295d8fbf531b095",
  "providers/notion.js": "db19d53a551f396083af0e1bb722a8db4460b318",
  "providers/oxalpha.js": "4d2b4daa8ed9a73725017b85f239cf02c03ff613",
  "providers/qwen-net.js": "5c59017d38a6378998d81c70642d22a665c85685",
  "providers/qwen.js": "8e4bf0e4056d8231a23297706d83ddda1ec49e43",
  "providers/useai.js": "13fb689507b47ed8251650504f995db165613c1a"
};
const requiredProviders = [
  "providers/deepseek.js",
  "providers/chatgpt.js",
  "providers/claude.js",
  "providers/gemini.js",
  "providers/kimi.js",
  "providers/glm.js",
  "providers/qwen.js",
  "providers/arena.js",
  "providers/freebuff.js",
  "providers/meta.js",
  "providers/copilot.js",
  "providers/crax.js",
  "providers/useai.js",
  "providers/oxalpha.js",
  "providers/notion.js"
];

function gitBlobSha(path) {
  const body = fs.readFileSync(path);
  const header = Buffer.from("blob " + body.length + "\0");
  return crypto.createHash("sha1").update(header).update(body).digest("hex");
}

for (const [path, expected] of Object.entries(baseline)) {
  if (!fs.existsSync(path)) throw new Error("Protected PlazCode file missing: " + path);
  const actual = gitBlobSha(path);
  if (actual !== expected) throw new Error("Protected PlazCode file changed: " + path + "\nexpected " + expected + "\nactual   " + actual);
}

for (const path of requiredProviders) {
  if (!fs.existsSync(path)) throw new Error("Supported provider missing: " + path);
}

const manifest = JSON.parse(fs.readFileSync("manifest.json", "utf8"));
if (manifest.version !== "1.18.60") throw new Error("Expected extension version 1.18.60");
for (const entry of manifest.content_scripts || []) {
  for (const path of entry.js || []) {
    if (!fs.existsSync(path)) throw new Error("Manifest references missing JS: " + path);
  }
  for (const path of entry.css || []) {
    if (!fs.existsSync(path)) throw new Error("Manifest references missing CSS: " + path);
  }
}

const background = fs.readFileSync("background.js", "utf8");
for (const token of [
  "DESKTOP_PREF_KEYS",
  "/api/preferences",
  "_plazcodePersisted",
  "initialDesktopPreferencesSync",
  "pullDesktopPreferences",
  "pushDesktopPreferences",
  "desktop_tools_snapshot",
  "/api/tools/browser",
  "chrome.runtime.onMessage.addListener",
  "sendResponse"
]) {
  if (!background.includes(token)) throw new Error("Desktop relay contract missing: " + token);
}

const main = fs.readFileSync("core/main.js", "utf8");
for (const token of [
  "desktopBrowserTools",
  "syncDesktopToolSnapshot",
  "SWEEP_ACTIVE_MS",
  "SWEEP_IDLE_MS",
  "releaseMediaUrl",
  "clearMediaFiles",
  "nextDelay = document.hidden ? 1000",
  "e.t - lastDiagDomAt >= 2000"
]) {
  if (!main.includes(token)) throw new Error("Long-chat/tool catalog regression guard missing: " + token);
}

if (main.includes("barRaf = requestAnimationFrame(placeBar);")) {
  throw new Error("High-frequency perpetual placeBar requestAnimationFrame loop returned");
}
if (main.includes("rsInterval(scheduleSweep, 1500)")) {
  throw new Error("Old high-frequency fallback sweep returned");
}

const providerNames = requiredProviders.map((p) => p.split("/").pop());
console.log("desktop regression: protected", Object.keys(baseline).length, "core/provider files");
console.log("desktop regression: supported provider adapters present:", providerNames.join(", "));
console.log("desktop regression: notion remains a known live-site exception; its adapter is still hash-protected");
console.log("desktop regression: PASS");
