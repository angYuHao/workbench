import { useEffect } from "react";
import { Form, FormItem, Input, InputNumber, Modal, message, useForm } from "../../ui";
import { useSettingsStore } from "../../stores/settings";
import type { DesktopRule } from "../../types";

interface Props {
  /** 正在编辑的那条；id 为 0 表示新增 */
  rule: DesktopRule;
  onClose: () => void;
}

/** 只在弹窗打开时挂载，关掉就整体销毁，不用手动 resetFields */
export default function RuleEditModal({ rule, onClose }: Props) {
  const saveRule = useSettingsStore((s) => s.saveRule);
  const [form] = useForm();

  useEffect(() => {
    form.setFieldsValue({
      category: rule.category,
      keywords: rule.keywords ?? "",
      extensions: rule.extensions ?? "",
      sort: rule.sort,
    });
  }, [rule, form]);

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      await saveRule({
        ...rule,
        category: values.category,
        keywords: values.keywords?.trim() || null,
        extensions: values.extensions?.trim() || null,
        sort: values.sort ?? 0,
      });
      message.success("已保存");
      onClose();
    } catch (e) {
      // 只有表单校验的错要在这里说；保存本身的错由 store 抛出、ModalConfirm 之外没人接
      if (String(e).includes("必填")) message.error(String(e));
    }
  };

  return (
    <Modal title={rule.id ? "编辑规则" : "新增规则"} open onCancel={onClose} onOk={handleSave} okText="保存">
      <Form form={form}>
        <FormItem name="category" label="目标分类名" rules={[{ required: true }]}>
          <Input placeholder="如：办公软件" />
        </FormItem>
        <FormItem name="keywords" label="关键词（逗号分隔）">
          <Input placeholder="word,excel,ppt" />
        </FormItem>
        <FormItem name="extensions" label="扩展名（逗号分隔，含点）">
          <Input placeholder=".docx,.pdf,.xlsx" />
        </FormItem>
        <FormItem name="sort" label="排序">
          <InputNumber min={0} style={{ width: "100%" }} />
        </FormItem>
      </Form>
    </Modal>
  );
}
