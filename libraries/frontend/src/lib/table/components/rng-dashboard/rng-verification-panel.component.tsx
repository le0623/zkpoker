import type { RngMetadata } from '../../types/rng.types';

type VerificationStatus = 'idle' | 'pending' | 'verified' | 'failed';

interface RngVerificationPanelProps {
  verificationStatus: VerificationStatus;
  onVerify: () => void;
  rngData?: RngMetadata;
}

export function RngVerificationPanel({
  verificationStatus,
  onVerify,
  rngData,
}: RngVerificationPanelProps) {
  const handleExportProof = () => {
    if (!rngData) return;

    const proofData = {
      round_id: rngData.round_id.toString(),
      timestamp: rngData.timestamp_ns.toString(),
      random_bytes: Array.from(rngData.raw_random_bytes),
      time_seed: rngData.time_seed.toString(),
      deck_hash: rngData.deck_hash,
      shuffled_deck: rngData.shuffled_deck.map((card, index) => ({
        position: index,
        value: Object.keys(card.value)[0],
        suit: Object.keys(card.suit)[0],
      })),
      verification_info: {
        message: 'Use this data to independently verify the shuffle',
        instructions: [
          '1. Extract random_bytes and time_seed',
          '2. Apply Fisher-Yates shuffle algorithm',
          '3. Calculate SHA-256 hash of shuffled deck',
          '4. Compare with deck_hash',
          '5. If match: shuffle is fair!',
        ],
      },
    };

    const blob = new Blob([JSON.stringify(proofData, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `rng-proof-round-${rngData.round_id}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const handleExportForVerification = () => {
    if (!rngData) return;

    const verificationScript = `// RNG Verification Script
// Round ID: ${rngData.round_id}
// Timestamp: ${rngData.timestamp_ns}

// Random bytes from IC VRF
const randomBytes = [${rngData.raw_random_bytes.join(', ')}];
const timeSeed = ${rngData.time_seed}n;
const expectedHash = "${rngData.deck_hash}";

// Verification steps:
// 1. Apply Fisher-Yates shuffle with randomBytes + timeSeed
// 2. Calculate SHA-256 hash of shuffled deck
// 3. Compare with expectedHash
// 4. If match: ✅ Fair shuffle!

console.log("Expected hash:", expectedHash);
console.log("Use a SHA-256 library to verify the shuffle");

// Example with crypto (Node.js):
// const crypto = require('crypto');
// const hash = crypto.createHash('sha256');
// // ... hash each card in shuffled order ...
// const calculatedHash = hash.digest('hex');
// console.log("Match:", calculatedHash === expectedHash);
`;

    const blob = new Blob([verificationScript], { type: 'text/javascript' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `verify-round-${rngData.round_id}.js`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };
  return (
    <section className="rng-panel verification-panel">
      <h3 className="panel-title">✅ Verification</h3>

      <div className="verification-content">
        <div className="verification-status">
          {verificationStatus === 'verified' && (
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

          {verificationStatus === 'failed' && (
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

          {verificationStatus === 'pending' && (
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

          {verificationStatus === 'idle' && (
            <div className="status-idle">
              <div className="status-message">
                Click the button to verify that the shuffle was fair.
              </div>
            </div>
          )}
        </div>

        <div className="verification-buttons">
          <button
            onClick={onVerify}
            disabled={verificationStatus === 'pending'}
            className="verify-button"
          >
            {verificationStatus === 'pending' ? 'Verifying...' : 'Verify Shuffle'}
          </button>

          {rngData && (
            <div className="export-buttons">
              <button
                onClick={handleExportProof}
                className="export-button"
                title="Export proof data for independent verification"
              >
                📥 Export Proof
              </button>
              <button
                onClick={handleExportForVerification}
                className="export-button"
                title="Export verification script"
              >
                🔍 Export Script
              </button>
            </div>
          )}
        </div>

        <div className="verification-explanation">
          <p className="info-hint">
            <strong>How verification works:</strong>
          </p>
          <ol className="verification-steps">
            <li>Take the original random bytes (from IC VRF)</li>
            <li>Apply time seed for additional entropy</li>
            <li>Re-shuffle the deck using Fisher-Yates algorithm</li>
            <li>Calculate SHA-256 hash of the re-shuffled deck</li>
            <li>Compare with the stored hash (committed before dealing)</li>
            <li>If they match: ✅ Fair shuffle! (No tampering possible)</li>
          </ol>
        </div>

        <div className="transparency-checklist">
          <p className="info-hint">
            <strong>What proves transparency:</strong>
          </p>
          <ul className="checklist-items">
            <li>✅ Random bytes from Internet Computer VRF (unpredictable)</li>
            <li>✅ Deck hash committed BEFORE any cards dealt (no cheating possible)</li>
            <li>✅ Each card has a committed hash (52 hashes visible in deck order)</li>
            <li>✅ Revealed cards can be verified against their committed hash</li>
            <li>✅ Full deck order visible after game ends (complete audit)</li>
            <li>✅ Independent verification (anyone can verify the hash)</li>
            <li>✅ Immutable on-chain storage (cannot be modified)</li>
          </ul>
        </div>

        <div className="hash-verification-explanation">
          <p className="info-hint">
            <strong>🔒 Hash-Based Verification:</strong>
          </p>
          <ul className="checklist-items">
            <li>All 52 card hashes are visible during gameplay (in deck order)</li>
            <li>Each hash is: SHA-256(round_id || card || position)</li>
            <li>Revealed cards show both value and hash (hover to see full hash)</li>
            <li>After game ends, verify each card's value matches its committed hash</li>
            <li>Any card swap would produce a different hash (impossible to hide)</li>
          </ul>
        </div>
      </div>
    </section>
  );
}

