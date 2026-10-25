ALTER TABLE issues ADD COLUMN status ENUM('unresolved', 'resolved', 'ignored') NOT NULL DEFAULT 'unresolved';
