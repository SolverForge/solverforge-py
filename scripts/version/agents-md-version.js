// commit-and-tag-version updater for the AGENTS.md release-responsibility line.
// The pre-publication and post-publication wordings are both matched so the
// surface keeps updating after the post-publication guidance commit.

const VERSION = String.raw`[0-9]+\.[0-9]+\.[0-9]+`;
const patterns = [
  new RegExp(String.raw`(prepares package/crate \`)(${VERSION})(\`)`, "g"),
  new RegExp(String.raw`(published package/crate version\s+is \`)(${VERSION})(\`)`, "g"),
];

module.exports.readVersion = function (contents) {
  for (const pattern of patterns) {
    pattern.lastIndex = 0;
    const match = pattern.exec(contents);
    if (match) {
      return match[2];
    }
  }
  throw new Error("package version not found in AGENTS.md");
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
    throw new Error("package version not found in AGENTS.md");
  }
  return updated;
};
