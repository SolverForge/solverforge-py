// commit-and-tag-version updater for the native crate version in Cargo.lock.
// Only the solverforge_py package block is a release surface; the SolverForge
// dependency entries are locked by the dependency upgrade pass.

const pattern = /(\[\[package\]\]\nname = "solverforge_py"\nversion = ")[^"]*(")/;

module.exports.readVersion = function (contents) {
  const match = contents.match(pattern);
  if (!match) {
    throw new Error("solverforge_py version not found in Cargo.lock");
  }
  return match[0].match(/version = "([^"]*)"/)[1];
};

module.exports.writeVersion = function (contents, version) {
  if (!pattern.test(contents)) {
    throw new Error("solverforge_py version not found in Cargo.lock");
  }
  return contents.replace(pattern, `$1${version}$2`);
};
