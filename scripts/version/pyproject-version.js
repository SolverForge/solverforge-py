// commit-and-tag-version updater for the Python package release version.
// `[project].version` is the canonical surface every other file must match.

const pattern = /(\[project\][\s\S]*?^version = ")[^"]*(")/m;

module.exports.readVersion = function (contents) {
  const match = contents.match(pattern);
  if (!match) {
    throw new Error("package version not found in pyproject.toml");
  }
  return match[0].match(/version = "([^"]*)"/)[1];
};

module.exports.writeVersion = function (contents, version) {
  if (!pattern.test(contents)) {
    throw new Error("package version not found in pyproject.toml");
  }
  return contents.replace(pattern, `$1${version}$2`);
};
