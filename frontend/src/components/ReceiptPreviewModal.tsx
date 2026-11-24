import React from "react";
import { RenderedReceipt } from "../types";

interface ReceiptPreviewModalProps {
  receipt: RenderedReceipt | null;
  qrUrl: string | null;
  onClose: () => void;
}

export const ReceiptPreviewModal: React.FC<ReceiptPreviewModalProps> = ({
  receipt,
  qrUrl,
  onClose,
}) => {
  if (!receipt && !qrUrl) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1250,
        backgroundColor: "rgba(15, 23, 42, 0.8)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "1rem",
      }}
    >
      <div
        className="pos-card"
        style={{
          maxWidth: 540,
          width: "100%",
          maxHeight: "88vh",
          overflowY: "auto",
          display: "flex",
          flexDirection: "column",
          gap: "0.8rem",
        }}
      >
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <h3 style={{ margin: 0 }}>
            {receipt
              ? "🧾 ESC/POS Thermal Receipt Simulator (pos-hal)"
              : "📱 Signed Customer Self-Service QR Link"}
          </h3>
          <button className="pos-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        {receipt && (
          <>
            <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
              <span className="pos-badge">Job: {receipt.id}</span>
              <span className="pos-badge">{receipt.byte_length} ESC/POS bytes</span>
              {receipt.drawer_kicked && (
                <span className="pos-badge" style={{ color: "var(--pos-accent)" }}>
                  ⚡ RJ-11 Cash Drawer Pulsed
                </span>
              )}
            </div>

            <pre
              style={{
                backgroundColor: "#0b1120",
                color: "#f8fafc",
                padding: "0.9rem",
                borderRadius: "var(--pos-radius)",
                fontFamily: "'Courier New', monospace",
                fontSize: "0.8rem",
                overflowX: "auto",
                margin: 0,
                lineHeight: 1.4,
              }}
            >
              {receipt.plain_text_preview}
            </pre>

            <div>
              <div className="pos-muted" style={{ fontSize: "0.75rem", marginBottom: "0.25rem" }}>
                Raw ESC/POS Hex Stream (ESC @ init, alignment, bold, GS V cut):
              </div>
              <code
                style={{
                  display: "block",
                  fontSize: "0.7rem",
                  wordBreak: "break-all",
                  backgroundColor: "var(--pos-bg)",
                  padding: "0.5rem",
                  borderRadius: "var(--pos-radius)",
                }}
              >
                {receipt.escpos_hex.slice(0, 260)}...
              </code>
            </div>
          </>
        )}

        {qrUrl && (
          <div style={{ display: "flex", flexDirection: "column", gap: "0.6rem" }}>
            <p className="pos-muted" style={{ margin: 0, fontSize: "0.84rem" }}>
              HMAC-SHA256 signed token strictly scoped to this check (deny-by-default isolation—cannot view or pay any other table).
            </p>
            <input className="pos-input" readOnly value={qrUrl} />
            <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end" }}>
              <a
                href={qrUrl}
                target="_blank"
                rel="noreferrer"
                className="pos-btn pos-btn-accent"
                style={{ textDecoration: "none" }}
              >
                Open Signed Customer View ↗
              </a>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
