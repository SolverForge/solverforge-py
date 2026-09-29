// commit-and-tag-version updater for the native crate release version.
// Only the `[package]` version moves; `[package.metadata.solverforge]` pins the
// SolverForge dependency base and is owned by the dependency upgrade pass.

const pattern = /(\[package\][\s\S]*?^version = ")[^"]*(")/m;

module.exports.readVersion = function (contents) {
  const match = contents.match(pattern);
  if (!match) {
    throw new Error("package version not found in Cargo.toml");
  }
  return match[0].match(/version = "([^"]*)"/)[1];
};

module.exports.writeVersion = function (contents, version) {
  if (!pattern.test(contents)) {
    throw new Error("package version not found in Cargo.toml");
  }
  return contents.replace(pattern, `$1${version}$2`);
};
