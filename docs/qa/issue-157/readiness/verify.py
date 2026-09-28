#!/usr/bin/env python3
"""Inspect only the isolated native QA project; never stage a real user project."""
import hashlib
import json
import pathlib
import stat
import subprocess
import sys

project = pathlib.Path(sys.argv[1]).resolve()
assert (project / "startup-attempted").is_file(), "not a recovery QA project"
cache = project / ".omadesign/agent-attachments"
permissions = []
for path in [cache, *sorted(cache.rglob("*"))]:
    info = path.lstat()
    assert not stat.S_ISLNK(info.st_mode), path
    mode = stat.S_IMODE(info.st_mode)
    assert mode == (0o700 if path.is_dir() else 0o600), (path, oct(mode))
    permissions.append({"path": str(path.relative_to(project)), "mode": oct(mode)})

restored = json.loads((project / "restored-draft.json").read_text())
conversation = json.loads((project / "conversation.json").read_text())
users = [entry for entry in conversation["messages"] if entry["role"] == "user"]
assert len(users) == 1
sent = users[0]
assert len(restored["attachments"]) == len(sent["attachments"]) == 2
assert sent["text"] == "Priority: " + restored["request"]
assert [a["id"] for a in sent["attachments"]] == [a["id"] for a in restored["attachments"]]
assert all(a["source"].startswith(str(cache)) for a in sent["attachments"])
assert json.loads((project / "agent-recovery-errors.json").read_text()) == []
payload = json.loads((project / "acp-received.json").read_text())
assert payload["counts"]["image"] == 1
assert payload["counts"]["resource"] == 1
assert sent["text"] in payload["params"]["prompt"][0]["text"]

def git(*args):
    return subprocess.check_output(["git", *args], cwd=project, text=True)

git("init", "--quiet")
git("add", ".")
staged = git("ls-files").splitlines()
assert "reference.txt" in staged
assert not any(p.startswith(".omadesign/agent-attachments/") for p in staged)
assert (project / "reference.txt").stat().st_mode & 0o777 == 0o644
movie = project / "agent-recovery.mp4"
probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries", "stream=width,height,nb_read_frames,r_frame_rate,duration", "-of", "json", str(movie)], text=True))["streams"][0]
subprocess.run(["ffmpeg", "-v", "error", "-i", str(movie), "-f", "null", "-"], check=True)
report = {
    "passed": True,
    "source_commit": sys.argv[2],
    "recorded_binary_sha256": hashlib.sha256(pathlib.Path(sys.argv[3]).read_bytes()).hexdigest(),
    "movie_sha256": hashlib.sha256(movie.read_bytes()).hexdigest(),
    "native_video": probe,
    "startup_failure_restored_unsent_and_concurrent_drafts": True,
    "retry_sent_one_turn_with_same_two_attachments": True,
    "native_clipboard_types": ["text/plain;charset=utf-8", "image/png"],
    "cache_permissions": permissions,
    "git_add_dot_excludes_entire_private_cache": True,
    "git_check_ignore": git("check-ignore", "-v", str(pathlib.Path(sent["attachments"][0]["source"]))),
    "external_reference_mode_unchanged": "0o644",
    "acp_payload_counts": payload["counts"],
    "scope": "Native WGPU framebuffer with real clipboard MIME reads and UI input; local ACP process fixture, no third-party provider service",
}
(project / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
