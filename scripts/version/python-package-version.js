// commit-and-tag-version updater for the importable package version.
// `solverforge.__version__` must equal pyproject.toml for imports and built
// distributions to agree.

const pattern = /(^__version__ = ")[^"]*(")/m;

module.exports.readVersion = function (contents) {
  const match = contents.match(pattern);
  if (!match) {
    throw new Error("__version__ not found in python/solverforge/__init__.py");
  }
  return match[0].match(/__version__ = "([^"]*)"/)[1];
};

module.exports.writeVersion = function (contents, version) {
  if (!pattern.test(contents)) {
    throw new Error("__version__ not found in python/solverforge/__init__.py");
  }
  return contents.replace(pattern, `$1${version}$2`);
};
