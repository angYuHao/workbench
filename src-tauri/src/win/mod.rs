//! 手写 Win32 的那几层：注册表、shell、快捷方式、图标、抓屏、DPAPI、CNG、单实例、桌面根目录、盘符、回收站。
//! 常量以本机 SDK 头文件为准，别照记忆写。
pub mod clipboard;
pub mod cng;
pub mod desktop;
pub mod dpapi;
pub mod drive;
pub mod icon;
pub mod lnk;
pub mod registry;
pub mod screen;
pub mod recycle;
pub mod shell;
pub mod single_instance;
