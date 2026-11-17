-- ffprobe is no longer a tool: mpv reads technical details. Forget any path set for it.
DELETE FROM settings WHERE key = 'tool.ffprobe';
