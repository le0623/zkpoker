import type { RngMetadata } from "../../types/rng.types";

type VerificationStatus = "idle" | "pending" | "verified" | "failed";

interface RngVerificationPanelProps {
  verificationStatus: VerificationStatus;
  onVerify: () => void;
  rngData?: RngMetadata;
  gameEnded?: boolean;
}

export function RngVerificationPanel({
  verificationStatus,
  onVerify,
  rngData,
  gameEnded = false,
}: RngVerificationPanelProps) {
 

   
  return (
    <section className="rng-panel verification-panel">
      <h3 className="panel-title">✅ Verification</h3>

      <div className="verification-content">
        <div className="verification-status">
          {verificationStatus === "verified" && (
            <div className="status-verified">
              <span className="status-icon">✅</span>
              <div>
                <div className="status-title">Shuffle Verified!</div>
                <div className="status-message">
                  The deck hash matches. This shuffle was fair and has not been
                  tampered with.
                </div>
              </div>
            </div>
          )}

          {verificationStatus === "failed" && (
            <div className="status-failed">
              <span className="status-icon">❌</span>
              <div>
                <div className="status-title">Verification Failed</div>
                <div className="status-message">
                  The deck hash does not match. Please contact support.
                </div>
              </div>
            </div>
          )}

          {verificationStatus === "pending" && (
            <div className="status-pending">
              <span className="status-icon">⏳</span>
              <div>
                <div className="status-title">Verifying...</div>
                <div className="status-message">
                  Re-computing deck hash from random bytes...
                </div>
              </div>
            </div>
          )}

          {verificationStatus === "idle" && (
            <div className="status-idle">
              <div className="status-message">
                {gameEnded
                  ? "Click the button to verify that the shuffle was fair."
                  : "Verification will be available after the game ends."}
              </div>
            </div>
          )}
        </div>

        <div className="verification-buttons">
          <button
            onClick={onVerify}
            disabled={verificationStatus === "pending" || !gameEnded}
            className="verify-button"
            title={
              gameEnded
                ? "Verify that the shuffle was fair"
                : "Verification available after game ends"
            }
          >
            {verificationStatus === "pending"
              ? "Verifying..."
              : "Verify Shuffle"}
          </button>

         
        </div>

        <div className="verification-explanation">
          <p className="info-hint">
            <strong>How verification works:</strong>
          </p>
          <p
            className="info-hint"
            style={{ fontSize: "0.8125rem", marginBottom: "0.5rem" }}
          >
            <strong>🔒 Note:</strong> Verification is only available after the
            game ends to ensure the commit-reveal security model. During
            gameplay, only the deck hash and random bytes are visible.
          </p>
          <ol className="verification-steps">
            <li>Wait until the game ends (button becomes enabled)</li>
            <li>Take the original random bytes (from IC VRF)</li>
            <li>
              Apply time seed for additional entropy (revealed after game ends)
            </li>
            <li>Re-shuffle the deck using Fisher-Yates algorithm</li>
            <li>Calculate SHA-256 hash of the re-shuffled deck</li>
            <li>Compare with the stored hash (committed before dealing)</li>
            <li>If they match: ✅ Fair shuffle! (No tampering possible)</li>
          </ol>
        </div>

        <div className="hash-verification-explanation">
          <p className="info-hint">
            <strong>🔒 Hash-Based Verification:</strong>
          </p>
          <ul className="checklist-items">
            <li>
              All 52 card hashes are visible during gameplay (in deck order)
            </li>
            <li>Each hash is: SHA-256(round_id || card || position)</li>
            <li>
              Revealed cards show both value and hash (hover to see full hash)
            </li>
            <li>
              After game ends, verify each card's value matches its committed
              hash
            </li>
            <li>
              Any card swap would produce a different hash (impossible to hide)
            </li>
          </ul>
        </div>
      </div>
    </section>
  );
}
