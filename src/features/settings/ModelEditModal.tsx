import { useEffect, useState } from "react";
import { Button, Form, FormItem, Input, Modal, Select, message, useForm } from "../../ui";
import { listRemoteAiModels, probeAiModel, saveAiModel } from "../../api/aiModels";
import { HintView, LabelRow, fetchedAt, type Hint } from "./shared";
import type { AiModel } from "../../types";

interface Props {
  /** 正在编辑的那条；新增时传的是 blankModel（id 为 0） */
  model: AiModel;
  onClose: () => void;
  /** 后端会把拉到的模型列表顺手存进这条配置，所以拉完要通知外面重读一次 */
  onModelsChanged: () => void;
}

/**
 * 新增/编辑一个 OpenAI 兼容接口。
 * 远端列表、探测结果这些状态全在这里：弹窗关掉就该没，不留到下次。
 */
export default function ModelEditModal({ model, onClose, onModelsChanged }: Props) {
  const [modelForm] = useForm();
  const [remote, setRemote] = useState<string[]>(model.remoteModels);
  const [remoteHint, setRemoteHint] = useState<Hint | null>(
    // 拉回来的列表存在本机，点开编辑直接用；要最新的才点「获取模型」
    model.remoteModels.length
      ? { ok: true, text: `本机存着上次获取的 ${model.remoteModels.length} 个（${fetchedAt(model.remoteAt)}），要最新的点「获取模型」` }
      : null
  );
  const [probeHint, setProbeHint] = useState<Hint | null>(null);
  const [fetching, setFetching] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    modelForm.setFieldsValue({
      name: model.name,
      baseUrl: model.baseUrl,
      model: model.model,
      apiKey: "",
    });
  }, [model, modelForm]);

  const handleSave = async () => {
    try {
      const v = await modelForm.validateFields();
      await saveAiModel({
        id: model.id,
        name: v.name,
        baseUrl: v.baseUrl,
        model: v.model,
        apiKey: (v.apiKey ?? "").trim() || null,
        sort: model.sort,
      });
      message.success("已保存");
      onModelsChanged();
      onClose();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg && msg !== "[object Object]") message.error(msg);
    }
  };

  /** 表单里的地址、模型名和密钥：留空的项由后端退回库里存着的那份 */
  const formTarget = () => ({
    // 新增时 id 为 0；0 发过去会被当成一条已存配置的编号
    id: model.id || null,
    baseUrl: String(modelForm.getFieldValue("baseUrl") ?? "").trim(),
    apiKey: String(modelForm.getFieldValue("apiKey") ?? "").trim() || null,
    model: String(modelForm.getFieldValue("model") ?? "").trim() || null,
  });

  /** 手工填过的名字不在接口列表里时，也要在下拉框中看得见 */
  const modelOptions = () => {
    const current = String(modelForm.getFieldValue("model") ?? "").trim();
    const ids = current && !remote.includes(current) ? [...remote, current] : remote;
    return ids.map((id) => ({ label: id, value: id }));
  };

  const handleFetchRemote = async () => {
    const target = formTarget();
    if (!target.baseUrl) {
      setRemoteHint({ ok: false, text: "先填接口地址" });
      return;
    }
    setFetching(true);
    setRemoteHint(null);
    try {
      const list = await listRemoteAiModels(target);
      setRemote(list);
      setRemoteHint(
        list.length
          ? { ok: true, text: `取到 ${list.length} 个模型，在下面的下拉框里选，已存本机` }
          : { ok: false, text: "接口没返回模型列表，模型名要自己填" }
      );
      if (target.id) onModelsChanged();
    } catch (e) {
      setRemote([]);
      setRemoteHint({ ok: false, text: String(e) });
    } finally {
      setFetching(false);
    }
  };

  const handleProbe = async () => {
    const target = formTarget();
    if (!target.baseUrl) {
      setProbeHint({ ok: false, text: "先填接口地址" });
      return;
    }
    setTesting(true);
    setProbeHint(null);
    try {
      const r = await probeAiModel(target);
      setProbeHint({ ok: r.ok, text: r.message });
    } catch (e) {
      setProbeHint({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Modal
      title={model.id ? "编辑模型" : "添加模型"}
      open
      onCancel={onClose}
      onOk={handleSave}
      okText="保存"
      width={520}
    >
      <Form form={modelForm}>
        <FormItem name="name" label="显示名" rules={[{ required: true }]}>
          <Input placeholder="自己起个名，如：内网模型" />
        </FormItem>
        <FormItem name="baseUrl" label="接口地址" rules={[{ required: true }]}>
          <Input placeholder="https://…/v1（OpenAI 兼容，填到 v1 为止）" />
        </FormItem>
        <FormItem
          name="model"
          label={
            <LabelRow
              text="模型名"
              action={
                <span className="settings-row-actions">
                  {remote.length > 0 && (
                    <Button type="link" size="small" onClick={() => setRemote([])}>
                      手填
                    </Button>
                  )}
                  <Button type="link" size="small" loading={fetching} onClick={handleFetchRemote}>
                    获取模型列表
                  </Button>
                </span>
              }
            />
          }
          rules={[{ required: true }]}
        >
          {remote.length > 0 ? (
            <Select
              allowClear
              showSearch
              searchPlaceholder="输入关键字过滤，回车选第一个"
              placeholder="从接口返回的列表里选"
              options={modelOptions()}
            />
          ) : (
            <Input placeholder="接口那边叫什么就填什么" />
          )}
        </FormItem>
        <HintView msg={remoteHint} />
        <FormItem
          name="apiKey"
          label={
            <LabelRow
              text={model.hasKey ? "密钥（留空保持不变）" : "密钥"}
              action={
                <Button type="link" size="small" loading={testing} onClick={handleProbe}>
                  测试调用
                </Button>
              }
            />
          }
        >
          <Input type="password" placeholder="sk-…" />
        </FormItem>
        <HintView msg={probeHint} />
      </Form>
    </Modal>
  );
}
