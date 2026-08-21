import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const IDENTITY = "Kyra Stable Local Development";
const KEYCHAIN = join(
  homedir(),
  "Library",
  "Keychains",
  "kyra-stable-signing.keychain-db",
);

if (process.platform !== "darwin") {
  process.exit(0);
}

function run(command, args, { capture = false } = {}) {
  return execFileSync(command, args, {
    encoding: "utf8",
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
}

function identityIsReady() {
  if (!existsSync(KEYCHAIN)) return false;
  try {
    return run(
      "security",
      ["find-identity", "-v", "-p", "codesigning", KEYCHAIN],
      { capture: true },
    ).includes(`\"${IDENTITY}\"`);
  } catch {
    return false;
  }
}

function addToUserSearchList() {
  const current = run("security", ["list-keychains", "-d", "user"], {
    capture: true,
  });
  const paths = [...current.matchAll(/\"([^\"]+)\"/g)].map((match) => match[1]);
  if (!paths.includes(KEYCHAIN)) {
    run("security", ["list-keychains", "-d", "user", "-s", ...paths, KEYCHAIN]);
  }
}

function createIdentity() {
  const certificateDirectory = mkdtempSync(join(tmpdir(), "kyra-signing-"));
  const privateKey = join(certificateDirectory, "private-key.pem");
  const certificate = join(certificateDirectory, "certificate.pem");
  const identity = join(certificateDirectory, "identity.p12");

  try {
    run("openssl", [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-keyout",
      privateKey,
      "-out",
      certificate,
      "-days",
      "3650",
      "-subj",
      `/CN=${IDENTITY}`,
      "-addext",
      "basicConstraints=critical,CA:TRUE",
      "-addext",
      "keyUsage=critical,digitalSignature,keyCertSign",
      "-addext",
      "extendedKeyUsage=codeSigning",
    ]);
    run("openssl", [
      "pkcs12",
      "-export",
      "-legacy",
      "-inkey",
      privateKey,
      "-in",
      certificate,
      "-out",
      identity,
      "-passout",
      "pass:kyra-local-import",
    ]);
    run("security", [
      "import",
      identity,
      "-k",
      KEYCHAIN,
      "-P",
      "kyra-local-import",
      "-T",
      "/usr/bin/codesign",
      "-T",
      "/usr/bin/security",
    ]);
    run("security", [
      "set-key-partition-list",
      "-S",
      "apple-tool:,apple:,codesign:",
      "-s",
      "-k",
      "",
      KEYCHAIN,
    ]);
    run("security", [
      "add-trusted-cert",
      "-r",
      "trustRoot",
      "-p",
      "codeSign",
      "-k",
      KEYCHAIN,
      certificate,
    ]);
  } finally {
    rmSync(certificateDirectory, { recursive: true, force: true });
  }
}

const createdKeychain = !existsSync(KEYCHAIN);
if (createdKeychain) {
  run("security", ["create-keychain", "-p", "", KEYCHAIN]);
}

run("security", ["unlock-keychain", "-p", "", KEYCHAIN]);
if (createdKeychain) {
  run("security", ["set-keychain-settings", "-lut", "21600", KEYCHAIN]);
}
addToUserSearchList();

if (!identityIsReady()) {
  createIdentity();
}

if (!identityIsReady()) {
  throw new Error(`macOS signing identity ${IDENTITY} is unavailable`);
}

console.log(`Using stable macOS signing identity: ${IDENTITY}`);
