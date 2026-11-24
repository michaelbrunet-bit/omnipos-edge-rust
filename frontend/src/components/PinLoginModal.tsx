import React, { useState } from "react";
import { OperatorPublicView, Role } from "../types";

interface PinLoginModalProps {
  isOpen: boolean;
  operators: OperatorPublicView[];
  initialRole: Role;
  scopedOrderId: string;
  onClose: () => void;
  onAuthenticate: (role: Role, pin?: string, scopedOrderId?: string) => Promise<void>;
}

export const PinLoginModal: React.FC<PinLoginModalProps> = ({
  isOpen,
  operators,
  initialRole,
  scopedOrderId,
  onClose,
  onAuthenticate,
}) => {
  const [role, setRole] = useState<Role>(
    initialRole === "customer" ? "bartender" : initialRole
  );
  const [pin, setPin] = useState<string>("");
  const [busy, setBusy] = useState<boolean>(false);
  const [err, setErr] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      await onAuthenticate(
        role,
        role === "customer" ? undefined : pin,
        role === "customer" ? scopedOrderId : undefined
      );
      setPin("");
      onClose();
    } catch (error) {
      setErr(error instanceof Error ? error.message : "Authentication failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1200,
        backgroundColor: "rgba(15, 23, 42, 0.78)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "1rem",
      }}
    >
      <div
        className="pos-card"
        style={{
          maxWidth: 440,
          width: "100%",
          display: "flex",
          flexDirection: "column",
          gap: "0.85rem",
        }}
      >
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <h3 style={{ margin: 0 }}>🔐 Argon2id Operator PIN Terminal</h3>
          <button className="pos-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        <p className="pos-muted" style={{ margin: 0, fontSize: "0.82rem" }}>
          Verifies salted Argon2id credentials (with 5-attempt brute-force lockout) and issues an HMAC-SHA256 signed session token.
        </p>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: "0.45rem",
          }}
        >
          {operators.map((op) => (
            <button
              key={op.id}
              type="button"
              className={`pos-btn ${
                role === op.role ? "pos-btn-primary" : ""
              }`}
              style={{
                flexDirection: "column",
                alignItems: "flex-start",
                padding: "0.5rem",
                fontSize: "0.76rem",
              }}
              onClick={() => {
                setRole(op.role);
                setPin(op.demo_pin_hint);
                setErr(null);
              }}
            >
              <strong>{op.role.toUpperCase()}</strong>
              <span>{op.name.split(" (")[0]}</span>
              <span className="pos-muted">Demo PIN: {op.demo_pin_hint}</span>
            </button>
          ))}
        </div>

        <form
          onSubmit={handleSubmit}
          style={{ display: "flex", flexDirection: "column", gap: "0.65rem" }}
        >
          <input
            type="password"
            inputMode="numeric"
            className="pos-input"
            placeholder="Enter 4-digit PIN (e.g. 1111, 9999, 2222, 3333)..."
            value={pin}
            onChange={(e) => setPin(e.target.value)}
            autoFocus
          />

          {err && (
            <div style={{ color: "#ef4444", fontSize: "0.82rem", fontWeight: 600 }}>
              ⚠️ {err}
            </div>
          )}

          <div style={{ display: "flex", justifyContent: "flex-end", gap: "0.5rem" }}>
            <button type="button" className="pos-btn" onClick={onClose}>
              Cancel
            </button>
            <button
              type="submit"
              className="pos-btn pos-btn-primary"
              disabled={busy || !pin.trim()}
            >
              {busy ? "Verifying Argon2id..." : "Unlock Signed Session"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
