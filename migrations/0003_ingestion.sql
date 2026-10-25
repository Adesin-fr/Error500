CREATE TABLE issues (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    project_id BIGINT UNSIGNED NOT NULL,
    fingerprint CHAR(64) NOT NULL,
    title VARCHAR(512) NOT NULL,
    level VARCHAR(16) NOT NULL,
    first_seen DATETIME(3) NOT NULL,
    last_seen DATETIME(3) NOT NULL,
    event_count BIGINT UNSIGNED NOT NULL DEFAULT 0,
    UNIQUE KEY issues_fingerprint (project_id, fingerprint),
    KEY issues_last_seen (project_id, last_seen),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;

CREATE TABLE events (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    project_id BIGINT UNSIGNED NOT NULL,
    issue_id BIGINT UNSIGNED NOT NULL,
    event_id CHAR(32) NOT NULL,
    level VARCHAR(16) NOT NULL,
    title VARCHAR(512) NOT NULL,
    message TEXT NULL,
    platform VARCHAR(64) NULL,
    release_name VARCHAR(255) NULL,
    environment VARCHAR(255) NULL,
    transaction_name VARCHAR(512) NULL,
    exception_type VARCHAR(255) NULL,
    exception_value TEXT NULL,
    occurred_at DATETIME(3) NOT NULL,
    payload LONGTEXT NOT NULL,
    UNIQUE KEY events_event_id (project_id, event_id),
    KEY events_issue (issue_id, occurred_at),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;

CREATE TABLE logs (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    project_id BIGINT UNSIGNED NOT NULL,
    occurred_at DATETIME(3) NOT NULL,
    level VARCHAR(16) NOT NULL,
    body TEXT NOT NULL,
    trace_id CHAR(32) NULL,
    attributes LONGTEXT NULL,
    KEY logs_time (project_id, occurred_at),
    KEY logs_trace (project_id, trace_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;

CREATE TABLE replays (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    project_id BIGINT UNSIGNED NOT NULL,
    replay_id CHAR(32) NOT NULL,
    first_seen DATETIME(3) NOT NULL,
    last_seen DATETIME(3) NOT NULL,
    urls LONGTEXT NULL,
    release_name VARCHAR(255) NULL,
    environment VARCHAR(255) NULL,
    UNIQUE KEY replays_replay_id (project_id, replay_id),
    KEY replays_last_seen (project_id, last_seen),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;

CREATE TABLE replay_segments (
    project_id BIGINT UNSIGNED NOT NULL,
    replay_id CHAR(32) NOT NULL,
    segment_id INT UNSIGNED NOT NULL,
    blob_key VARCHAR(512) NOT NULL,
    size_bytes BIGINT UNSIGNED NOT NULL,
    PRIMARY KEY (project_id, replay_id, segment_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;
