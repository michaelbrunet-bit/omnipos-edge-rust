import React, { useState } from "react";
import { posApi } from "../api";

interface ManagerOverrideModalProps {
  requiredAction: string | null;
  onCancel: () => void;
  onApproved: (overrideToken: string, managerName: string) => Promise<void>;
}

export const ManagerOverrideModal: React.FC<ManagerOverrideModalProps> = ({
  requiredAction,
  onCancel,
  onApproved,
}) => {
  const [pin, setPin] = useState<string>("");
  const [busy, setBusy] = useState<boolean>(false);
  const [err, setErr] = useState<string | null>(null);

  if (!requiredAction) return null;

  const handleApprove = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      const res = await posApi.requestManagerOverride(pin, requiredAction);
      setPin("");
      await onApproved(res.override_token, res.manager_name);
    } catch (error) {
      setErr(
        error instanceof Error
          ? error.message
          : "Invalid Manager or Owner PIN"
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1300,
        backgroundColor: "rgba(15, 23, 42, 0.82)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "1rem",
      }}
    >
      <div
        className="pos-card"
        style={{
          maxWidth: 420,
          width: "100%",
          border: "2px solid var(--pos-primary)",
          display: "flex",
          flexDirection: "column",
          gap: "0.8rem",
        }}
      >
        <div>
          <span className="pos-badge">🛡️ STEP-UP AUTHORIZATION REQUIRED</span>
          <h3 style={{ margin: "0.4rem 0 0.2rem 0" }}>
            Manager Override: {requiredAction}
          </h3>
          <p className="pos-muted" style={{ margin: 0, fontSize: "0.82rem" }}>
            Enter a Floor Manager PIN (<code>9999</code>) or Store Owner PIN (
            <code>1111</code>) to mint a single-action{" "}
            <code>X-Manager-Override</code> signature.
          </p>
        </div>

        <form
          onSubmit={handleApprove}
          style={{ display: "flex", flexDirection: "column", gap: "0.65rem" }}
        >
          <div style={{ display: "flex", gap: "0.4rem" }}>
            <input
              type="password"
              inputMode="numeric"
              className="pos-input"
              placeholder="Manager (9999) or Owner (1111) PIN..."
              value={pin}
              onChange={(e) => setPin(e.target.value)}
              autoFocus
            />
            <button
              type="button"
              className="pos-btn"
              onClick={() => setPin("9999")}
              title="Fill Demo Manager PIN 9999"
            >
              Use 9999
            </button>
          </div>

          {err && (
            <div style={{ color: "#ef4444", fontSize: "0.82rem", fontWeight: 600 }}>
              ⚠️ {err}
            </div>
          )}

          <div style={{ display: "flex", justifyContent: "flex-end", gap: "0.5rem" }}>
            <button type="button" className="pos-btn" onClick={onCancel}>
              Cancel
            </button>
            <button
              type="submit"
              className="pos-btn pos-btn-primary"
              disabled={busy || !pin.trim()}
            >
              {busy ? "Signing Override..." : "Approve Action"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
