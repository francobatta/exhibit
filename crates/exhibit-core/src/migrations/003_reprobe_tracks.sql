-- Probing now reads Dolby Vision, each sound track's profile (DTS-HD, DTS:X, Atmos) and the
-- tracks' flags (original, commentary, forced, SDH). Files read before are read again; a file
-- whose new read fails keeps the details it had.
UPDATE files SET probed = 0 WHERE probed = 1 AND tech IS NOT NULL;
