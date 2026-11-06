#!/bin/bash
# Release-directory helpers for deploy/install.sh. Sourced, never run; deploy/install.test.sh tests
# them with fake artifacts (no cargo, no launchctl).
#
# Layout under an install root (default ~/.local/opt/investing-app):
#
#   releases/<UTC-ts>-<shortsha>/   bin/investing-app, web/dist/, run.sh, RELEASE
#   current  -> releases/<id>       what launchd runs (the plist starts current/run.sh)
#   previous -> releases/<id>       what `install.sh --rollback` goes back to
#
# Symlinks are relative, so the root can be moved. Every function returns non-zero on failure and
# leaves `current` as it was; none of them relies on `set -e` (callers may test them in `if`).

# release_id <checkout> -> 20261106T134800Z-d1e245f (plus -dirty when tracked files differ from HEAD)
release_id() {
  local src="$1" sha suffix=""
  sha="$(git -C "$src" rev-parse --short=7 HEAD 2>/dev/null)" || sha="nogit"
  if [ "$sha" != "nogit" ] && [ -n "$(git -C "$src" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    suffix="-dirty"
  fi
  printf '%s-%s%s\n' "$(date -u +%Y%m%dT%H%M%SZ)" "$sha" "$suffix"
}

# release_link <root> <current|previous> -> the release id the link points at, or nothing
release_link() {
  local target
  target="$(readlink "$1/$2" 2>/dev/null)" || return 0
  printf '%s\n' "${target#releases/}"
}

# release_check <dir> <bin name>: the three things run.sh needs are present
release_check() {
  local dir="$1" name="$2"
  [ -x "$dir/bin/$name" ] || { echo "release check: $dir/bin/$name missing or not executable" >&2; return 1; }
  [ -f "$dir/web/dist/index.html" ] || { echo "release check: $dir/web/dist/index.html missing" >&2; return 1; }
  [ -x "$dir/run.sh" ] || { echo "release check: $dir/run.sh missing or not executable" >&2; return 1; }
}

# release_stage <root> <id> <bin> <web dist dir> <run.sh> [sha]
# Copies into releases/<id>.tmp, checks the copy, then renames it to releases/<id>. On any failure
# the .tmp directory is removed and releases/<id> does not exist.
release_stage() {
  local root="$1" id="$2" bin="$3" dist="$4" runsh="$5" sha="${6:-unknown}"
  local name final tmp
  name="$(basename "$bin")"
  final="$root/releases/$id"
  tmp="$final.tmp"
  [ -n "$id" ] || { echo "release stage: empty release id" >&2; return 1; }
  [ ! -e "$final" ] || { echo "release stage: $final already exists" >&2; return 1; }
  mkdir -p "$root/releases" || return 1
  rm -rf "$tmp"
  if ! { mkdir -p "$tmp/bin" "$tmp/web" &&
    cp -p "$bin" "$tmp/bin/$name" &&
    cmp -s "$bin" "$tmp/bin/$name" &&
    cp -Rp "$dist" "$tmp/web/dist" &&
    cp -p "$runsh" "$tmp/run.sh" &&
    chmod 755 "$tmp/run.sh" &&
    release_check "$tmp" "$name" &&
    printf 'id=%s\nsha=%s\nbuilt_at=%s\nstaged_at=%s\n' "$id" "$sha" \
      "$(date -u -r "$bin" +%Y-%m-%dT%H:%M:%SZ)" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$tmp/RELEASE" &&
    mv "$tmp" "$final"; }; then
    rm -rf "$tmp"
    echo "release stage: copy or check failed; current is unchanged" >&2
    return 1
  fi
}

# _release_point <root> <link name> <id>: atomic swap (new symlink at a temp name, then rename).
# mv -h renames over the link itself instead of moving into the directory it points at.
_release_point() {
  local root="$1" link="$2" id="$3" tmp="$1/.$2.new"
  rm -f "$tmp"
  ln -s "releases/$id" "$tmp" && mv -fh "$tmp" "$root/$link"
}

# release_activate <root> <id>: current -> id; the old current becomes previous.
# Returns 1 when `current` did not move, and 2 when it moved but `previous` could not be set: then
# `current` is the new release, so a caller must not delete it.
release_activate() {
  local root="$1" id="$2" old
  [ -d "$root/releases/$id" ] || { echo "release activate: no release $id" >&2; return 1; }
  old="$(release_link "$root" current)"
  _release_point "$root" current "$id" || return 1
  if [ -n "$old" ] && [ "$old" != "$id" ] && [ -d "$root/releases/$old" ]; then
    _release_point "$root" previous "$old" || {
      echo "release activate: current -> $id, but previous could not be set to $old" >&2
      return 2
    }
  fi
}

# release_rollback <root> <bin name>: current -> previous, previous -> the release rolled back from
# (so a second rollback goes forward again).
release_rollback() {
  local root="$1" name="$2" cur prev
  cur="$(release_link "$root" current)"
  prev="$(release_link "$root" previous)"
  [ -n "$prev" ] || { echo "rollback: no previous release in $root" >&2; return 1; }
  release_check "$root/releases/$prev" "$name" || { echo "rollback: previous release $prev is incomplete" >&2; return 1; }
  _release_point "$root" current "$prev" || return 1
  if [ -n "$cur" ] && [ -d "$root/releases/$cur" ]; then
    _release_point "$root" previous "$cur" || return 1
  fi
}

# release_prune <root> <keep>: keep current, previous and the newest others up to <keep> releases
# in total; delete the rest and any leftover *.tmp directory.
release_prune() {
  local root="$1" keep="$2" cur prev kept=0 name
  [ -d "$root/releases" ] || return 0
  cur="$(release_link "$root" current)"
  prev="$(release_link "$root" previous)"
  [ -z "$cur" ] || kept=$((kept + 1))
  [ -z "$prev" ] || [ "$prev" = "$cur" ] || kept=$((kept + 1))
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    case "$name" in
      *.tmp) rm -rf "${root:?}/releases/$name"; continue ;;
    esac
    if [ "$name" = "$cur" ] || [ "$name" = "$prev" ]; then continue; fi
    if [ "$kept" -lt "$keep" ]; then
      kept=$((kept + 1))
    else
      rm -rf "${root:?}/releases/${name:?}" || return 1
    fi
  done < <(find "$root/releases" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort -r)
}
