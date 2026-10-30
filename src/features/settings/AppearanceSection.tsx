import React, { useState } from "react";
import { Dices, Download, Palette, RotateCcw, Save, Upload, X } from "lucide-react";
import { Button, Input, Modal, ModalConfirm, TextArea, message } from "../../ui";
import { ZOOM_STEPS, useAppearanceStore } from "../../stores/appearance";
import { resolveMode } from "../../styles/apply";
import { contrastReport, deriveTokens } from "../../styles/derive";
import {
  DENSITY_LABELS,
  DENSITY_STEPS,
  INK_LABELS,
  INK_STEPS,
  MODE_LABELS,
  MODE_STEPS,
  PRESETS,
  RADIUS_LABELS,
  RADIUS_STEPS,
  RAIL_LABELS,
  RAIL_STEPS,
  SHADOW_LABELS,
  SHADOW_STEPS,
  TEXTURE_LABELS,
  TEXTURE_STEPS,
  TINT_LABELS,
  TINT_STEPS,
  seedOf,
  type DensityStep,
  type InkStep,
  type Mode,
  type RadiusStep,
  type RailStep,
  type ShadowStep,
  type StyleSeed,
  type TextureStep,
  type TintStep,
} from "../../styles/seeds";
import { copyText } from "../../utils/clipboard";
import { SettingsLine } from "./shared";

function Chips<T extends string | number>({
  value,
  options,
  label,
  onPick,
}: {
  value: T;
  options: readonly T[];
  label: (v: T) => string;
  onPick: (v: T) => void;
}) {
  return (
    <div className="ap-chips">
      {options.map((o) => (
        <button key={String(o)} type="button" className={o === value ? "ap-chip on" : "ap-chip"} onClick={() => onPick(o)}>
          {label(o)}
        </button>
      ))}
    </div>
  );
}

/** 预设卡片：把这一套种子派生成变量表贴在卡片内层，外面那圈 .ap-* 规则读的还是同一批变量名。
 *  所以预览和真实界面走的是同一条渲染路径 —— 不会出现「预览图好看、装上不是那样」的那天。 */
function StylePreview({
  seed,
  name,
  note,
  active,
  onPick,
}: {
  seed: StyleSeed;
  name: string;
  note?: string;
  active: boolean;
  onPick: () => void;
}) {
  const tokens = deriveTokens(seed) as unknown as React.CSSProperties;
  return (
    <button type="button" className={active ? "ap-item on" : "ap-item"} onClick={onPick} title={note}>
      <span className="ap-shot" style={tokens}>
        <span className="ap-rail">
          <i className="ap-dot" />
          <i className="ap-dot on" />
          <i className="ap-dot" />
        </span>
        <span className="ap-main">
          <span className="ap-bar">
            <i className="ap-title" />
            <i className="ap-btn" />
          </span>
          <span className="ap-body">
            <span className="ap-side">
              <i className="ap-row on" />
              <i className="ap-row" />
              <i className="ap-row" />
            </span>
            <span className="ap-cnt">
              <i className="ap-card" />
              <i className="ap-card" />
            </span>
          </span>
        </span>
      </span>
      <span className="ap-name">{name}</span>
    </button>
  );
}

/** 删掉存下来的风格没法撤销，走一次确认 */
function confirmDelete(name: string, onOk: () => void) {
  ModalConfirm({ title: `删除「${name}」？`, content: "只删这条本机存下来的风格，正在应用的那套不受影响。", onOk });
}

/** 外观设置：风格预设 + 档位覆盖 + 对比度自检。改一处当场生效，没有保存按钮 */
export default function AppearanceSection() {
  const store = useAppearanceStore();
  const seed = store.seed();
  const mode = resolveMode(seed.mode);
  const report = contrastReport(seed);
  const [name, setName] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [draft, setDraft] = useState("");

  const pickPreset = (id: string, label: string) => {
    void store.setStyle(id);
    message.success(`已换成「${label}」`);
  };

  const save = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      message.error("先给这套风格起个名");
      return;
    }
    await store.saveStyle(trimmed);
    setName("");
    message.success(`已存下「${trimmed}」`);
  };

  const copyOut = async () => {
    if (await copyText(store.exportJson())) message.success("风格 JSON 已复制，发给谁都能贴进来");
    else message.error("复制没成功，手动选中内容再来一次");
  };

  return (
    <>
      <div className="settings-card">
        <div className="ap-block">
          <div className="ap-block-head">
            风格预设
            <span className="ap-block-tip">点一张整套换掉；下面那些档位是它的默认值，可以接着改</span>
          </div>
          <div className="ap-grid">
            {PRESETS.map((p) => (
              <StylePreview
                key={p.id}
                seed={seedOf(p, mode, {})}
                name={p.name}
                note={p.note}
                active={store.styleId === p.id}
                onPick={() => pickPreset(p.id, p.name)}
              />
            ))}
          </div>
        </div>

        {!!store.userStyles.length && (
          <div className="ap-block">
            <div className="ap-block-head">
              我的风格
              <span className="ap-block-tip">存下来的那几套，鼠标移到卡片上出现删除</span>
            </div>
            <div className="ap-grid">
              {store.userStyles.map((u) => (
                <span className="ap-cell" key={u.id}>
                  <StylePreview
                    seed={{ ...u.seed, mode }}
                    name={u.name}
                    active={store.styleId === u.id}
                    onPick={() => pickPreset(u.id, u.name)}
                  />
                  <button
                    type="button"
                    className="ap-del"
                    aria-label={`删除${u.name}`}
                    onClick={() => confirmDelete(u.name, () => void store.removeStyle(u.id))}
                  >
                    <X size={11} strokeWidth={2} />
                  </button>
                </span>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="settings-card">
        <SettingsLine icon={Palette} title="明暗" desc="跟随系统会实时跟着 Windows 那一档换">
          <Chips
            value={seed.mode}
            options={MODE_STEPS}
            label={(v) => MODE_LABELS[v]}
            onPick={(v) => void store.setMode(v as Mode)}
          />
        </SettingsLine>

        <SettingsLine title="主色" desc="越亮的主色越吃对比度，右边那几项读数会自动压到达标">
          <span className="ap-accent">
            <input type="color" value={seed.accent} aria-label="主色" onChange={(e) => store.patch({ accent: e.target.value })} />
            <span className="ap-hex">{seed.accent}</span>
            <Button size="small" onClick={() => store.patch({ accent: undefined })}>
              跟随预设
            </Button>
          </span>
        </SettingsLine>

        <SettingsLine title="圆角">
          <Chips value={seed.radius} options={RADIUS_STEPS} label={(v) => RADIUS_LABELS[v]} onPick={(v) => store.patch({ radius: v as RadiusStep })} />
        </SettingsLine>
        <SettingsLine title="密度" desc="改的是外壳的间距、栏内边距和控件高度；各功能页内部的排布暂时不跟着变">
          <Chips value={seed.density} options={DENSITY_STEPS} label={(v) => DENSITY_LABELS[v]} onPick={(v) => store.patch({ density: v as DensityStep })} />
        </SettingsLine>
        <SettingsLine title="界面缩放" desc="整块界面等比缩放，字体和按钮一起变小或变大 —— 嫌页面里那些写死字号的地方太大就用这条，密度管不到它们">
          <Chips value={store.zoom} options={ZOOM_STEPS} label={(v) => `${Math.round(v * 100)}%`} onPick={(v) => store.setZoom(v)} />
        </SettingsLine>
        <SettingsLine title="文字浓度" desc="淡 = 整条文字灰阶往面上提一档，浓 = 往黑/白压一档；描边和斑马纹跟着一起挪">
          <Chips value={seed.ink} options={INK_STEPS} label={(v) => INK_LABELS[v]} onPick={(v) => store.patch({ ink: v as InkStep })} />
        </SettingsLine>
        <SettingsLine title="阴影">
          <Chips value={seed.shadow} options={SHADOW_STEPS} label={(v) => SHADOW_LABELS[v]} onPick={(v) => store.patch({ shadow: v as ShadowStep })} />
        </SettingsLine>
        <SettingsLine title="染力度" desc="0 是全中性灰阶，2 是描边、斑马纹、阴影都掺进主色相 —— 「显年轻」主要靠这一档">
          <Chips value={seed.tint} options={TINT_STEPS} label={(v) => TINT_LABELS[v]} onPick={(v) => store.patch({ tint: v as TintStep })} />
        </SettingsLine>
        <SettingsLine title="侧边导轨" desc="深色那一档就是改版前的样子，留着当对照">
          <Chips value={seed.rail} options={RAIL_STEPS} label={(v) => RAIL_LABELS[v]} onPick={(v) => store.patch({ rail: v as RailStep })} />
        </SettingsLine>
        <SettingsLine title="背景质感">
          <Chips value={seed.texture} options={TEXTURE_STEPS} label={(v) => TEXTURE_LABELS[v]} onPick={(v) => store.patch({ texture: v as TextureStep })} />
        </SettingsLine>
      </div>

      <div className="settings-card">
        <div className="ap-block">
          <div className="ap-block-head">
            对比度自检
            <span className="ap-block-tip">按 WCAG 算：正文 4.5:1、大字与非文本 3:1</span>
          </div>
          <div className="ap-checks">
            {report.map((r) => (
              <span key={r.label} className={r.value >= r.target ? "ap-check ok" : "ap-check bad"}>
                {r.label} {r.value.toFixed(2)}
              </span>
            ))}
          </div>
        </div>
      </div>

      <div className="settings-card">
        <SettingsLine title="存成我的风格" desc="把现在这套档位连手改的主色一起存下来，以后点卡片直接切回">
          <span className="ap-actions">
            <Input value={name} onChange={setName} placeholder="名字" style={{ width: 120 }} />
            <Button size="small" type="primary" icon={<Save size={14} strokeWidth={1.6} />} onClick={() => void save()}>
              保存
            </Button>
          </span>
        </SettingsLine>
        <SettingsLine title="分享" desc="复制这份配置的 JSON，或者把别人那份贴进来">
          <span className="ap-actions">
            <Button size="small" icon={<Download size={14} strokeWidth={1.6} />} onClick={() => void copyOut()}>
              复制 JSON
            </Button>
            <Button size="small" icon={<Upload size={14} strokeWidth={1.6} />} onClick={() => setImportOpen(true)}>
              贴入应用
            </Button>
          </span>
        </SettingsLine>
        <SettingsLine title="手风">
          <span className="ap-actions">
            <Button size="small" icon={<Dices size={14} strokeWidth={1.6} />} onClick={() => pickPreset(PRESETS[Math.floor(Math.random() * PRESETS.length)].id, "随机那一套")}>
              换一套
            </Button>
            <Button size="small" icon={<RotateCcw size={14} strokeWidth={1.6} />} onClick={() => void store.reset()}>
              回到预设默认
            </Button>
          </span>
        </SettingsLine>
      </div>

      <Modal title="贴入风格 JSON" open={importOpen} onCancel={() => setImportOpen(false)} width={520} footer={null}>
        <TextArea value={draft} onChange={setDraft} autoSize={{ minRows: 6, maxRows: 14 }} placeholder="把别人那份外观配置贴在这里" />
        <div className="ap-import-foot">
          <Button onClick={() => setImportOpen(false)}>取消</Button>
          <Button
            type="primary"
            onClick={() => {
              void (async () => {
                try {
                  await store.importJson(draft);
                  setImportOpen(false);
                  setDraft("");
                  message.success("已应用贴进来的风格");
                } catch (e) {
                  message.error(`这份读不出来：${String(e)}`);
                }
              })();
            }}
          >
            应用
          </Button>
        </div>
      </Modal>
    </>
  );
}
