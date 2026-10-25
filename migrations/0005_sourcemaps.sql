ALTER TABLE projects ADD COLUMN upload_token CHAR(32) NOT NULL DEFAULT '';

CREATE TABLE sourcemaps (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    project_id BIGINT UNSIGNED NOT NULL,
    release_key VARCHAR(190) NOT NULL DEFAULT '',
    name VARCHAR(512) NOT NULL,
    name_hash CHAR(40) NOT NULL,
    debug_id VARCHAR(64) NULL,
    blob_key VARCHAR(512) NOT NULL,
    size_bytes BIGINT UNSIGNED NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY sourcemaps_name (project_id, release_key, name_hash),
    KEY sourcemaps_debug_id (project_id, debug_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;
