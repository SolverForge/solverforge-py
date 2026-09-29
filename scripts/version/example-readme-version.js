// commit-and-tag-version updater for the example README release surfaces.
// The published-package note and the example install snippet must move
// together, and the source-checkout commands around them must not.

const VERSION = String.raw`[0-9]+\.[0-9]+\.[0-9]+`;
const patterns = [
  new RegExp(String.raw`(After \`)(${VERSION})(\` is published)`, "g"),
  new RegExp(String.raw`(against the published \`)(${VERSION})(\`)`, "g"),
  new RegExp(String.raw`(solverforge\[examples\]==)(${VERSION})(")`, "g"),
];

module.exports.readVersion = function (contents) {
  for (const pattern of patterns) {
    pattern.lastIndex = 0;
    const match = pattern.exec(contents);
    if (match) {
      return match[2];
    }
  }
  throw new Error("package version not found in the example README");
};

module.exports.writeVersion = function (contents, version) {
  let updated = contents;
  let replaced = false;
  for (const pattern of patterns) {
    pattern.lastIndex = 0;
    if (pattern.test(updated)) {
      pattern.lastIndex = 0;
      updated = updated.replace(pattern, `$1${version}$3`);
      replaced = true;
    }
  }
  if (!replaced) {
    throw new Error("package version not found in the example README");
  }
  return updated;
};
