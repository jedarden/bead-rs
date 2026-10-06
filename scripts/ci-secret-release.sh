#!/usr/bin/env bash
# Candidate packaging and guarded promotion. Called only after both CI lanes
# pass in the exact detached Argo clone; never run from a shared host checkout.
set -euo pipefail

requested_revision=${1:?full requested revision required}
publish_release=${2:-false}
[[ "$requested_revision" =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid release revision' >&2; exit 2; }
[[ "$publish_release" == false || "$publish_release" == true ]] || { echo 'Invalid publication mode' >&2; exit 2; }
[[ "$(git rev-parse HEAD)" == "$requested_revision" ]] || { echo 'Candidate revision mismatch' >&2; exit 1; }
[[ "${BEAD_RELEASE_CI:-}" == argo && "$PWD" == /workspace ]] || { echo 'Release packaging requires the isolated Argo workspace' >&2; exit 1; }
version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Invalid release version' >&2; exit 2; }

# No tag or release-side mutation is possible before the approval check.
if [[ "$publish_release" == true ]]; then
  python3 scripts/verify-secret-release.py "$version" --candidate-info > /tmp/approved-candidate.json
fi

assets_dir="$PWD/release-assets"
mkdir -p "$assets_dir"
[[ -z "$(find "$assets_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]] || { echo 'Candidate assets directory must start empty' >&2; exit 1; }
if [[ "$publish_release" == false ]]; then
export CARGO_TARGET_DIR="$PWD/target"
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
# Keep dependency caching, but rebuild the application so an earlier tree's
# cached build-script provenance cannot reach a new release candidate.
cargo clean --package bead-rs

for profile in default managed; do
  feature_args=()
  prefix=bead
  if [[ "$profile" == managed ]]; then
    feature_args=(--features managed-secret-policy)
    prefix=bead-managed
  fi
  cargo build --locked --release --bin bead --target x86_64-unknown-linux-gnu "${feature_args[@]}"
  cp target/x86_64-unknown-linux-gnu/release/bead "$assets_dir/$prefix-x86_64-unknown-linux-gnu"
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    cargo build --locked --release --bin bead --target aarch64-unknown-linux-gnu "${feature_args[@]}"
  cp target/aarch64-unknown-linux-gnu/release/bead "$assets_dir/$prefix-aarch64-unknown-linux-gnu"

  # Copy only this invocation's successful Darwin builds. A cache artifact
  # left by a failed build must never masquerade as this source revision.
  if CC=o64-clang CXX=o64-clang++ cargo build --locked --release --bin bead --target x86_64-apple-darwin "${feature_args[@]}"; then
    cp target/x86_64-apple-darwin/release/bead "$assets_dir/$prefix-x86_64-apple-darwin"
  else
    echo "Optional Darwin build unavailable ($profile/x86_64)"
  fi
  if CC=oa64-clang CXX=oa64-clang++ cargo build --locked --release --bin bead --target aarch64-apple-darwin "${feature_args[@]}"; then
    cp target/aarch64-apple-darwin/release/bead "$assets_dir/$prefix-aarch64-apple-darwin"
  else
    echo "Optional Darwin build unavailable ($profile/aarch64)"
  fi
done
cp install.sh "$assets_dir/install.sh"

# Test the exact packaged native managed binary, not a debug build or a pin.
"$assets_dir/bead-managed-x86_64-unknown-linux-gnu" capabilities > /tmp/managed-capabilities.json
jq -e '.. | objects | select(.compiled_policy? == "managed-enforce-no-ack")' /tmp/managed-capabilities.json >/dev/null
python3 scripts/smoke-secret-release.py "$assets_dir/bead-managed-x86_64-unknown-linux-gnu" --scratch-root /tmp
"$assets_dir/bead-managed-x86_64-unknown-linux-gnu" --version
for native_binary in bead-x86_64-unknown-linux-gnu bead-managed-x86_64-unknown-linux-gnu; do
  native_version=$("$assets_dir/$native_binary" --version)
  short_revision=$(git rev-parse --short HEAD)
  [[ "$native_version" == "bead $version ($short_revision "* ]] || { echo 'Packaged source/version provenance mismatch' >&2; exit 1; }
done

inventory=$(find "$assets_dir" -maxdepth 1 -type f -name 'bead-*' -printf '%f\n' | LC_ALL=C sort | jq -Rsc 'split("\n") | map(select(length > 0))')
jq -n --arg source "$requested_revision" --arg version "$version" \
  --arg rust "$(rustc --version)" --arg builder "$BUILDER_IMAGE" \
  --argjson artifacts "$inventory" \
  '{schema_version:1,source_commit:$source,version:$version,builder_image:$builder,rustc:$rust,
    profiles:{default:[],managed:["managed-secret-policy"]},binaries:$artifacts}' > "$assets_dir/provenance.json"
(cd "$assets_dir" && sha256sum bead-* install.sh provenance.json > checksums.txt && sha256sum --strict -c checksums.txt)
for name in bead-x86_64-unknown-linux-gnu bead-aarch64-unknown-linux-gnu \
            bead-managed-x86_64-unknown-linux-gnu bead-managed-aarch64-unknown-linux-gnu install.sh provenance.json; do
  [[ -s "$assets_dir/$name" ]] || { echo 'Required release artifact missing' >&2; exit 1; }
done
# The controller has no default artifact repository. Retain candidates in the
# existing private application CI registry, using a distinct immutable semver
# candidate tag (never a Cargo cache key or a mutable deployment image).
candidate_ref="ronaldraygun/bead-rs-ci-cargo-cache:$version-candidate.$requested_revision"
if ! candidate_digest=$(crane digest "$candidate_ref" 2>/dev/null); then
  candidate_layer=$(mktemp /tmp/bead-release-candidate.XXXXXXXX.tar)
  tar -C "$assets_dir" -cf "$candidate_layer" .
  crane append -f "$candidate_layer" -t "$candidate_ref"
  candidate_digest=$(crane digest "$candidate_ref")
fi
[[ "$candidate_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || { echo 'Candidate digest unavailable' >&2; exit 1; }
crane export "${candidate_ref%:*}@$candidate_digest" - | python3 scripts/verify-release-candidate.py "$assets_dir/checksums.txt"
printf '%s@%s\n' "${candidate_ref%:*}" "$candidate_digest" > /tmp/candidate-digest
echo "Verified candidate retained: $candidate_ref@$candidate_digest"
echo "Candidate v$version built; publication is disabled"
exit 0
else
  # Promotion never rebuilds. Restore only the immutable payload whose exact
  # bytes were replayed and whose source tree matches this approval checkout.
  release_revision=$(jq -er '.source_commit' /tmp/approved-candidate.json)
  candidate_payload=$(jq -er '.payload_ref' /tmp/approved-candidate.json)
  approved_manifest=$(jq -er '.checksums_path' /tmp/approved-candidate.json)
  crane export "$candidate_payload" - | python3 scripts/verify-release-candidate.py "$approved_manifest" --output-dir "$assets_dir"
  printf '%s\n' "$candidate_payload" > /tmp/candidate-digest
  jq -e --arg revision "$release_revision" --arg version "$version" \
    '.source_commit == $revision and .version == $version and .profiles.default == [] and .profiles.managed == ["managed-secret-policy"]' \
    "$assets_dir/provenance.json" >/dev/null
  chmod +x "$assets_dir"/bead-*
  python3 scripts/smoke-secret-release.py "$assets_dir/bead-managed-x86_64-unknown-linux-gnu" --scratch-root /tmp
  (cd "$assets_dir" && sha256sum --strict -c checksums.txt)
fi

# Plain push only to the configured Forgejo origin; never retarget an existing
# version. The approved candidate revision (not the later evidence commit)
# becomes the tag; the guard proved both contain the identical source tree.
[[ "$(git remote get-url origin)" == https://git.ardenone.com/jedarden/bead-rs.git ]] || { echo 'Untrusted release origin' >&2; exit 1; }
tag="v$version"
tag_sha=$(git ls-remote --tags origin "refs/tags/$tag" "refs/tags/$tag^{}" | tail -n1 | cut -f1)
if [[ -n "$tag_sha" ]]; then
  [[ "$tag_sha" == "$release_revision" ]] || { echo 'Release tag belongs to another revision' >&2; exit 1; }
else
  git tag "$tag" "$release_revision"
  git push origin "refs/tags/$tag"
fi

# Forgejo's mirror owns GitHub refs. --verify-tag forbids gh from creating a
# GitHub-only tag which the next mirror sync would prune.
mirrored=false
for attempt in {1..12}; do
  github_sha=$(gh api "repos/jedarden/bead-rs/git/ref/tags/$tag" --jq '.object.sha' 2>/dev/null || true)
  if [[ "$github_sha" == "$release_revision" ]]; then mirrored=true; break; fi
  sleep 5
done
[[ "$mirrored" == true ]] || { echo 'Authoritative tag has not mirrored yet; release remains unpublished' >&2; exit 1; }
assets=("$assets_dir"/bead-* "$assets_dir/install.sh" "$assets_dir/provenance.json" "$assets_dir/checksums.txt")
draft=$(gh release view "$tag" --repo jedarden/bead-rs --json isDraft --jq '.isDraft' 2>/dev/null || echo absent)
if [[ "$draft" == absent ]]; then
  gh release create "$tag" --repo jedarden/bead-rs --verify-tag --draft \
    --title "bead-rs $tag" --notes "Secret scrubbing release. Source: $release_revision. Fleet binaries use managed-secret-policy; see provenance.json and checksums.txt." \
    "${assets[@]}"
fi

# A new or pre-existing draft is published only after every uploaded byte has
# been checked. Never blindly promote a draft or overwrite existing assets.
expected_assets=$(find "$assets_dir" -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort | jq -Rsc 'split("\n") | map(select(length > 0))')
gh release view "$tag" --repo jedarden/bead-rs --json assets > /tmp/uploaded-release-assets.json
jq -e --argjson expected "$expected_assets" '.assets | map(.name) | sort == $expected' /tmp/uploaded-release-assets.json >/dev/null || { echo 'Uploaded release inventory differs; retained unpublished' >&2; exit 1; }
verify_dir=$(mktemp -d /tmp/bead-release-verification.XXXXXXXX)
gh release download "$tag" --repo jedarden/bead-rs --dir "$verify_dir" --pattern checksums.txt
cmp "$assets_dir/checksums.txt" "$verify_dir/checksums.txt" || { echo 'Existing release manifest differs; retained unpublished' >&2; exit 1; }
while read -r digest filename; do
  [[ "$digest" =~ ^[0-9a-f]{64}$ && "$filename" =~ ^(bead-[a-z0-9_-]+|install\.sh|provenance\.json)$ ]] || { echo 'Invalid uploaded checksum manifest' >&2; exit 1; }
  gh release download "$tag" --repo jedarden/bead-rs --dir "$verify_dir" --pattern "$filename"
done < "$verify_dir/checksums.txt"
(cd "$verify_dir" && sha256sum --strict -c checksums.txt)
if [[ "$draft" != false ]]; then
  gh release edit "$tag" --repo jedarden/bead-rs --draft=false
fi
gh release view "$tag" --repo jedarden/bead-rs --json isDraft --jq '.isDraft' | grep -qx false
echo "Published and verified bead-rs $tag from $release_revision"
