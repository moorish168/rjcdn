-- New domains should not advertise HTTP/3 until UDP reachability is configured.
-- Updating the column default leaves existing domains' explicit values unchanged.
ALTER TABLE domains
    ALTER COLUMN http3_enabled SET DEFAULT FALSE;
