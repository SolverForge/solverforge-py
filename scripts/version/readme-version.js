// commit-and-tag-version updater for the documented README release surfaces.
// The install snippet and the source-checkout example note must move together,
// and the SolverForge dependency versions in the same file must not move. Both
// the pre-publication and post-publication wordings are matched so the surface
// keeps updating after the post-publication guidance commit.

const VERSION = String.raw`[0-9]+\.[0-9]+\.[0-9]+`;
const patterns = [
  new RegExp(String.raw`(prepared by this checkout is \`solverforge\` \`)(${VERSION})(\`)`, "g"),
  new RegExp(String.raw`(published \`solverforge\` \`)(${VERSION})(\` package)`, "g"),
  new RegExp(String.raw`(solverforge==)(${VERSION})(")`, "g"),
  new RegExp(String.raw`(solverforge\[examples\]==)(${VERSION})(")`, "g"),
  new RegExp(String.raw`(package once \`)(${VERSION})(\` is released)`, "g"),
];

module.exports.readVersion = function (contents) {
  for (const pattern of patterns) {
    pattern.lastIndex = 0;
    const match = pattern.exec(contents);
    if (match) {
      return match[2];
    }
  }
  throw new Error("package version not found in README.md");
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
    throw new Error("package version not found in README.md");
  }
  return updated;
};
