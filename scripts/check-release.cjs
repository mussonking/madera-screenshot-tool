const fs = require("fs");

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, "utf8"));
}

function read(path) {
  return fs.readFileSync(path, "utf8");
}

function fail(message) {
  failures.push(message);
}

const failures = [];
const pkg = readJson("package.json");
const lock = readJson("package-lock.json");
const tauri = readJson("src-tauri/tauri.conf.json");
const cargoToml = read("src-tauri/Cargo.toml");
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const versions = {
  "package.json": pkg.version,
  "package-lock.json": lock.version,
  "package-lock root": lock.packages?.[""]?.version,
  "src-tauri/Cargo.toml": cargoVersion,
  "src-tauri/tauri.conf.json": tauri.version,
};

const expectedVersion = pkg.version;
for (const [source, version] of Object.entries(versions)) {
  if (version !== expectedVersion) {
    fail(`${source} version is ${version || "missing"}, expected ${expectedVersion}`);
  }
}

if (pkg.name !== "madera-ss") fail(`package.json name is ${pkg.name}, expected madera-ss`);
if (lock.name !== "madera-ss") fail(`package-lock.json name is ${lock.name}, expected madera-ss`);
if (lock.packages?.[""]?.name !== "madera-ss") {
  fail(`package-lock root name is ${lock.packages?.[""]?.name}, expected madera-ss`);
}
if (tauri.productName !== "Madera.SS") fail(`Tauri productName is ${tauri.productName}, expected Madera.SS`);
if (tauri.identifier !== "com.madera.ss") fail(`Tauri identifier is ${tauri.identifier}, expected com.madera.ss`);
if (!fs.existsSync("src-tauri/Cargo.lock")) fail("src-tauri/Cargo.lock is missing");
if (fs.existsSync(".claude/settings.local.json")) fail(".claude/settings.local.json should not be committed");

if (failures.length > 0) {
  console.error("Release readiness check failed:");
  for (const item of failures) console.error(`- ${item}`);
  process.exit(1);
}

console.log(`Release readiness check passed for Madera.SS ${expectedVersion}.`);
