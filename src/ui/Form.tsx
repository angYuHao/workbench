import React, { createContext, useContext, useMemo, useRef, useState } from "react";

interface FormInstance {
  getFieldValue: (name: string) => any;
  setFieldValue: (name: string, value: any) => void;
  setFieldsValue: (values: Record<string, any>) => void;
  resetFields: () => void;
  validateFields: () => Promise<Record<string, any>>;
  _values: React.MutableRefObject<Record<string, any>>;
  _rules: React.MutableRefObject<Record<string, any[]>>;
}

const FormContext = createContext<FormInstance | null>(null);

export function useForm(): [FormInstance] {
  const [, forceUpdate] = useState(0);
  const valuesRef = useRef<Record<string, any>>({});
  const rulesRef = useRef<Record<string, any[]>>({});

  // 实例必须只创建一次：视图把 form 放进 useEffect 依赖数组，
  // 若每次渲染都返回新对象，会与 resetFields() 的 setState 形成无限循环。
  const instance = useMemo<FormInstance>(
    () => ({
      _values: valuesRef,
      _rules: rulesRef,
      getFieldValue: (name) => valuesRef.current[name],
      setFieldValue: (name, value) => {
        valuesRef.current[name] = value;
        forceUpdate((n) => n + 1);
      },
      setFieldsValue: (values) => {
        valuesRef.current = { ...valuesRef.current, ...values };
        forceUpdate((n) => n + 1);
      },
      resetFields: () => {
        valuesRef.current = {};
        forceUpdate((n) => n + 1);
      },
      validateFields: () => {
        const errors: string[] = [];
        for (const [name, rules] of Object.entries(rulesRef.current)) {
          const val = valuesRef.current[name];
          for (const rule of rules) {
            if (rule.required && (val === undefined || val === null || val === "")) {
              errors.push(`${name} 为必填项`);
            }
            if (rule.type === "url" && val) {
              try {
                new URL(val);
              } catch {
                errors.push(`${name} 不是有效的网址`);
              }
            }
          }
        }
        if (errors.length) {
          return Promise.reject(new Error(errors[0]));
        }
        return Promise.resolve({ ...valuesRef.current });
      },
    }),
    [],
  );

  return [instance];
}

interface FormProps {
  form: FormInstance;
  layout?: "vertical" | "horizontal";
  children: React.ReactNode;
}

export function Form({ form, children }: FormProps) {
  return <FormContext.Provider value={form}>{children}</FormContext.Provider>;
}

interface FormItemProps {
  name: string;
  label?: React.ReactNode;
  rules?: any[];
  children: React.ReactElement;
}

export function FormItem({ name, label, rules, children }: FormItemProps) {
  const form = useContext(FormContext);
  if (form && rules) {
    form._rules.current[name] = rules;
  }

  const value = form?._values.current[name];
  const handleChange = (v: any) => {
    form?.setFieldValue(name, v);
  };

  const child = React.cloneElement(children, {
    value: value ?? "",
    onChange: handleChange,
  } as any);

  return (
    <div style={{ marginBottom: 12 }}>
      {label && (
        <label style={{ display: "block", marginBottom: 4, fontSize: 14, color: "var(--color-text)" }}>
          {label}
        </label>
      )}
      {child}
    </div>
  );
}