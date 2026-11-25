#!/usr/bin/env node
// Release candidates: every green main CI run publishes one as a GitHub prerelease. Full releases
// stay manual. Shared by Rubylight and Nightjar (client/.github/scripts/rc.mjs is the same file).
//
//   plan <version> <sha>            picks the RC of <sha>: vX.Y.Z-rc.N, where X.Y.Z is <version>, or the
//                                   first patch after it without a release tag, and N counts the commits
//                                   from the latest full release to <sha>. Rubylight and Nightjar share
//                                   their version, so one commit gets the same RC version in both.
//                                   Writes version, tag and exists (true when the repo has that tag).
//   notes <tag> <sha> <out>         writes the release text: the commits since the previous release,
//                                   RC or full (RC_PATHSPEC limits them to a part of the tree).
//   version-commit <sha> <version>  Nightjar: commits app/build.gradle with versionName <version> on
//                                   top of <sha>, outside any branch, so the RC reports its own
//                                   version. Writes commit.
//   prune                           deletes the RCs older than the latest full release, with their
//                                   tags, and failed RC drafts. Never touches a full release.
//
// Env: GITHUB_TOKEN, GITHUB_REPOSITORY (or RC_REPO for another repo), RC_INTRO (text above the commit list), RC_PATHSPEC
// (optional, space-separated git pathspecs), RC_DRY_RUN=true (prune only lists what it would delete).

import fs from "node:fs";
import { execFileSync } from "node:child_process";

const env = process.env;
const API = env.GITHUB_API_URL || "https://api.github.com";
const SERVER = env.GITHUB_SERVER_URL || "https://github.com";
const REPO = env.RC_REPO || env.GITHUB_REPOSITORY;
const RC_TAG = /^v(\d+)\.(\d+)\.(\d+)-rc\.(\d+)$/;
const VERSION = /^(\d+)\.(\d+)\.(\d+)$/;
const COMMITS_MAX = 150;

async function gh(route, { method = "GET", body, allow = [] } = {}) {
  const response = await fetch(API + route, {
    method,
    headers: {
      Authorization: `Bearer ${env.GITHUB_TOKEN}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "rubylight-rc",
      ...(body ? { "Content-Type": "application/json" } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (allow.includes(response.status)) return null;
  if (!response.ok) throw new Error(`${method} ${route}: ${response.status} ${await response.text()}`);
  return response.status === 204 ? null : response.json();
}

function output(name, value) {
  if (env.GITHUB_OUTPUT) fs.appendFileSync(env.GITHUB_OUTPUT, `${name}=${value}\n`);
  console.log(`${name}=${value}`);
}

async function releases() {
  const all = [];
  for (let page = 1; page <= 5; page++) {
    const batch = await gh(`/repos/${REPO}/releases?per_page=100&page=${page}`);
    all.push(...batch);
    if (batch.length < 100) break;
  }
  return all;
}

// Semver order for X.Y.Z and X.Y.Z-rc.N (an RC sorts below its release).
function key(tag) {
  const rc = tag.match(RC_TAG);
  if (rc) return [+rc[1], +rc[2], +rc[3], 0, +rc[4]];
  const full = tag.replace(/^v/, "").match(VERSION);
  return full ? [+full[1], +full[2], +full[3], 1, 0] : null;
}

function below(a, b) {
  for (let i = 0; i < 5; i++) if (a[i] !== b[i]) return a[i] < b[i];
  return false;
}

const isRc = (release) => RC_TAG.test(release.tag_name) && (release.prerelease || release.draft);
const isFull = (release) => !release.draft && !release.prerelease && key(release.tag_name)?.[3] === 1;
const newest = (list) => [...list].sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }).trim();
}

function hasCommit(ref) {
  try {
    git("rev-parse", "--verify", "--quiet", `${ref}^{commit}`);
    return true;
  } catch {
    return false;
  }
}

async function plan(version, sha) {
  const match = String(version).trim().match(VERSION);
  if (!match) throw new Error(`not an X.Y.Z version: ${version}`);
  if (!sha) throw new Error("plan needs the commit");
  // The first patch from <version> on without a release tag, here or in the repo.
  let patch = +match[3];
  const released = async (name) => hasCommit(`refs/tags/${name}`) || Boolean(await gh(`/repos/${REPO}/git/ref/tags/${name}`, { allow: [404] }));
  while (await released(`v${match[1]}.${match[2]}.${patch}`)) patch++;
  const base = `${match[1]}.${match[2]}.${patch}`;
  let since = null;
  try {
    since = git("describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", "--exclude", "*-*", sha);
  } catch {
    // no full release in the history: count every commit
  }
  const n = Number(git("rev-list", "--count", since ? `${since}..${sha}` : sha));
  const tag = `v${base}-rc.${n}`;
  output("version", `${base}-rc.${n}`);
  output("tag", tag);
  output("exists", Boolean(await gh(`/repos/${REPO}/git/ref/tags/${tag}`, { allow: [404] })));
}

// The commits in from..to (all of `to` when from is null), without the RC version commits.
function commitsIn(from, to, pathspec) {
  const args = ["log", "--no-merges", "--invert-grep", "--grep=^chore(rc): version ", "--format=%h%x09%s"];
  if (!from) args.push("-n", "50");
  args.push(from ? `${from}..${to}` : to);
  if (pathspec.length) args.push("--", ...pathspec);
  return git(...args).split("\n").filter(Boolean).map((line) => {
    const [hash, ...subject] = line.split("\t");
    return `- ${subject.join("\t").replace(/@(?=\w)/g, "@​")} (${hash})`;
  });
}

// The changes since the previous release: the newest RC below this one, else the latest full
// release. Every RC since the latest full release is kept, so together they cover all changes.
async function notes(tag, sha, out) {
  const all = await releases();
  const full = newest(all.filter(isFull)).find((release) => hasCommit(release.tag_name));
  const pathspec = (env.RC_PATHSPEC || "").split(/\s+/).filter(Boolean);
  const current = key(tag);
  // Only published RCs: a failed RC's commits belong to the next one.
  const earlier = all
    .filter((release) => isRc(release) && !release.draft)
    .map((release) => release.tag_name)
    .filter((name) => below(key(name), current) && (!full || below(key(full.tag_name), key(name))) && hasCommit(name))
    .sort((a, b) => (below(key(a), key(b)) ? -1 : 1));
  const previous = earlier.at(-1) || full?.tag_name || null;
  let lines = commitsIn(previous, sha, pathspec);
  if (lines.length > COMMITS_MAX) lines = [...lines.slice(0, COMMITS_MAX), `- … and ${lines.length - COMMITS_MAX} more`];
  const body = [
    env.RC_INTRO || "",
    `Built from ${sha.slice(0, 10)} on main.`,
    previous ? `## Changes since ${previous}` : "## Changes",
    lines.length ? lines.join("\n") : "- No changes in this part of the tree.",
    previous ? `[Compare with ${previous}](${SERVER}/${REPO}/compare/${previous}...${tag})` : "",
    full && previous !== full.tag_name ? `[All changes since the last full release, ${full.tag_name}](${SERVER}/${REPO}/compare/${full.tag_name}...${tag})` : "",
  ].filter(Boolean).join("\n\n");
  fs.writeFileSync(out, `${body}\n`);
  output("commits", lines.length);
  console.log(`${lines.length} commits since ${previous || "the start"}`);
}

async function versionCommit(sha, version) {
  const file = "app/build.gradle";
  const content = await gh(`/repos/${REPO}/contents/${file}?ref=${sha}`);
  const text = Buffer.from(content.content, "base64").toString("utf8");
  const line = /^(\s*versionName ")[^"]*(")$/m;
  if (!line.test(text)) throw new Error(`no versionName line in ${file}`);
  const parent = await gh(`/repos/${REPO}/git/commits/${sha}`);
  const tree = await gh(`/repos/${REPO}/git/trees`, {
    method: "POST",
    body: { base_tree: parent.tree.sha, tree: [{ path: file, mode: "100644", type: "blob", content: text.replace(line, `$1${version}$2`) }] },
  });
  const commit = await gh(`/repos/${REPO}/git/commits`, {
    method: "POST",
    body: { message: `chore(rc): version ${version}`, tree: tree.sha, parents: [sha] },
  });
  output("commit", commit.sha);
}

// RCs live until the next full release: then they and their tags go. Failed RC drafts go too.
async function prune() {
  const all = await releases();
  const full = newest(all.filter(isFull))[0];
  const rcs = all.filter(isRc);
  const latest = newest(rcs.filter((release) => !release.draft))[0];
  const doomed = rcs.filter(
    (release) =>
      (full && below(key(release.tag_name), key(full.tag_name))) ||
      // A draft older than the newest published RC is one whose build failed or was replaced.
      (release.draft && latest && (below(key(release.tag_name), key(latest.tag_name)) || Date.parse(release.created_at) < Date.parse(latest.created_at))),
  );
  for (const release of doomed) {
    if (!isRc(release)) continue; // never a full release
    if (env.RC_DRY_RUN === "true") {
      console.log(`would delete ${release.tag_name}`);
      continue;
    }
    await gh(`/repos/${REPO}/releases/${release.id}`, { method: "DELETE", allow: [404] });
    await gh(`/repos/${REPO}/git/refs/tags/${release.tag_name}`, { method: "DELETE", allow: [404, 422] }).catch((error) =>
      console.log(`::warning::kept tag ${release.tag_name}: ${error.message}`),
    );
    console.log(`deleted ${release.tag_name}`);
  }
  console.log(`kept ${rcs.length - doomed.length} RCs`);
}

const [command, ...args] = process.argv.slice(2);
const run = { plan, notes, "version-commit": versionCommit, prune }[command];
if (!run) {
  console.error("usage: rc.mjs plan|notes|version-commit|prune ...");
  process.exit(2);
}
run(...args).catch((error) => {
  console.error(error.stack || String(error));
  process.exit(1);
});
