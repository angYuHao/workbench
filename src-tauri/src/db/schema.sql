-- 分类表（三大模块共用，type 区分隔离）
CREATE TABLE IF NOT EXISTS categories (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL,
  type       TEXT NOT NULL,
  parent_id  INTEGER,
  sort       INTEGER DEFAULT 0,
  icon       TEXT
);

-- 程序/桌面图标
CREATE TABLE IF NOT EXISTS apps (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,
  alias       TEXT,
  description TEXT,
  target      TEXT NOT NULL,
  args        TEXT,
  icon_path   TEXT,
  category_id INTEGER REFERENCES categories(id),
  source      TEXT DEFAULT 'manual',
  created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 「应用」页里被他删掉的程序：开始菜单同步时按 target 认出来，不再收回来
-- 只在用户点删除时记；撤销整理那种内部删行不算
CREATE TABLE IF NOT EXISTS dismissed_apps (
  target       TEXT PRIMARY KEY COLLATE NOCASE,
  dismissed_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 网址收藏
CREATE TABLE IF NOT EXISTS bookmarks (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL,
  url         TEXT NOT NULL,
  description TEXT,
  category_id INTEGER REFERENCES categories(id),
  source      TEXT DEFAULT 'manual',
  created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 文件/文件夹/项目
CREATE TABLE IF NOT EXISTS files (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  path         TEXT NOT NULL UNIQUE,
  name         TEXT NOT NULL,
  alias        TEXT,
  description  TEXT,
  is_dir       INTEGER DEFAULT 0,
  managed_root TEXT,
  category_id  INTEGER REFERENCES categories(id),
  created_at   DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 桌面整理日志（用于一键恢复）
CREATE TABLE IF NOT EXISTS organize_logs (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  file_name    TEXT,
  from_path    TEXT,
  to_path      TEXT,
  organized_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  reverted     INTEGER DEFAULT 0
);

-- 分类规则（桌面自动整理依据，可自定义）
CREATE TABLE IF NOT EXISTS desktop_rules (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  category    TEXT NOT NULL,
  keywords    TEXT,
  extensions  TEXT,
  enabled     INTEGER DEFAULT 1,
  sort        INTEGER DEFAULT 0
);

-- 设置键值
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT);

-- 2FA 账户。secret_enc 是用当前 Windows 用户加密（DPAPI）后的原始密钥，
-- 换机或换用户后无法解开，只能重新添加。
CREATE TABLE IF NOT EXISTS totp_accounts (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  issuer     TEXT NOT NULL,
  account    TEXT NOT NULL,
  secret_enc BLOB NOT NULL,
  algorithm  TEXT DEFAULT 'sha1',
  digits     INTEGER DEFAULT 6,
  period     INTEGER DEFAULT 30,
  sort       INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 自定义大模型（OpenAI 兼容接口）。key_enc 用当前 Windows 用户加密（DPAPI），
-- 只在本机能解开；接口地址与模型名都不预设，全由用户自己填。
CREATE TABLE IF NOT EXISTS ai_models (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL,
  base_url   TEXT NOT NULL,
  model      TEXT NOT NULL,
  key_enc    BLOB,
  sort       INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 网址黑名单：命中就不在网页管理里显示，导入浏览器收藏夹时也会跳过。
-- kind 分两种：url = 归一化后的精确地址（多选拉黑走这条），domain = 域名及其所有子域和页面（手输走这条）。
-- 匹配值入库前已经归一化（域名小写、去 www；地址小写主机但保留路径大小写），
-- 所以这里不再挂 COLLATE NOCASE —— 那会让 /A/B 和 /a/b 在库里偷偷并成一条。
CREATE TABLE IF NOT EXISTS bookmark_blacklist (
  pattern    TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

-- 网址智能分析删掉的重复书签整行存这儿，删错了能在网页管理里一键拿回来。
-- 只有分析删重复走这里，手动删书签不留底。
CREATE TABLE IF NOT EXISTS bookmark_trash (
  id          INTEGER PRIMARY KEY,
  title       TEXT NOT NULL,
  url         TEXT NOT NULL,
  description TEXT,
  category_id INTEGER,
  source      TEXT,
  deleted_at  INTEGER NOT NULL
);

-- 智能分类的本机记忆：查过一个软件就记下判断，下次不再问模型也不再联网。
-- 只存软件类的判断（文档、图片这些归分类规则管，不进这张表），一条就一句话。
CREATE TABLE IF NOT EXISTS ai_memory (
  name     TEXT PRIMARY KEY COLLATE NOCASE,
  category TEXT NOT NULL,
  evidence TEXT,
  source   TEXT NOT NULL,
  at       INTEGER NOT NULL
);

-- 使用计数：双击打开一次加一，用来把常用的排前面。
-- 单独一张表而不是往 apps/bookmarks 加列 —— 本项目建表只有 IF NOT EXISTS，老库里的表加不出新列。
-- kind 分 'app' 和 'bookmark'，因为两张表的 id 会撞号。
CREATE TABLE IF NOT EXISTS usage_stats (
  kind TEXT NOT NULL,
  id   INTEGER NOT NULL,
  uses INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (kind, id)
);

CREATE INDEX IF NOT EXISTS idx_categories_type ON categories(type);
CREATE INDEX IF NOT EXISTS idx_apps_category ON apps(category_id);
CREATE INDEX IF NOT EXISTS idx_bookmarks_category ON bookmarks(category_id);
CREATE INDEX IF NOT EXISTS idx_files_root ON files(managed_root);
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_totp_issuer ON totp_accounts(issuer);