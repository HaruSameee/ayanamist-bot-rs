CREATE TABLE dareda_result (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    pokemon_id INTEGER NOT NULL,
    is_correct INTEGER NOT NULL,
    attempts INTEGER NOT NULL,
    answered_at INTEGER NOT NULL
);

CREATE TABLE verify_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    result TEXT NOT NULL CHECK (result IN ('success', 'fail', 'timeout')),
    at INTEGER NOT NULL
);

CREATE TABLE member_join (
    user_id INTEGER NOT NULL,
    joined_at INTEGER NOT NULL,
    account_created_at INTEGER NOT NULL
);

-- ランキング・統計は answered_at で期間フィルタし user_id で集約するため
CREATE INDEX idx_dareda_result_answered_at ON dareda_result (answered_at);
CREATE INDEX idx_dareda_result_user ON dareda_result (user_id, answered_at);
-- verify_log はユーザー単位の参照を想定
CREATE INDEX idx_verify_log_user ON verify_log (user_id);
