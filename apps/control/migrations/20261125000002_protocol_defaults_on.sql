ALTER TABLE domains
    ALTER COLUMN http3_enabled SET DEFAULT TRUE,
    ALTER COLUMN image_webp_enabled SET DEFAULT TRUE;
