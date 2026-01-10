import { useState } from 'react';

export function RngTransparencyGuide() {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <section className="rng-panel transparency-guide">
      <div
        className="guide-header"
        onClick={() => setIsExpanded(!isExpanded)}
        style={{ cursor: 'pointer' }}
      >
        <h3 className="panel-title">📖 How to Verify Transparency</h3>
        <span className="expand-icon">{isExpanded ? '▼' : '▶'}</span>
      </div>

      {isExpanded && (
        <div className="guide-content">
          <div className="guide-section">
            <h4>Step 1: Check the Randomness Source</h4>
            <p>
              Verify that the random bytes come from <strong>Internet Computer VRF</strong>.
              This is the same cryptographic randomness used by the blockchain - it cannot be
              predicted or manipulated by anyone, including the house.
            </p>
          </div>

          <div className="guide-section">
            <h4>Step 2: Verify the Deck Hash</h4>
            <p>
              The <strong>Deck Hash</strong> was calculated and stored <strong>BEFORE</strong> any
              cards were dealt. This means:
            </p>
            <ul>
              <li>✅ The house cannot change cards after seeing what's dealt</li>
              <li>✅ The deck order is cryptographically committed</li>
              <li>✅ Any tampering would produce a different hash</li>
            </ul>
          </div>

          <div className="guide-section">
            <h4>Step 3: Click "Verify Shuffle" Button</h4>
            <p>
              This button re-computes the deck hash using the same random bytes and algorithm.
              If the hash matches the stored hash, the shuffle is proven fair.
            </p>
          </div>

          <div className="guide-section">
            <h4>Step 4: Check Revealed Cards (During Game)</h4>
            <p>
              During gameplay, only revealed cards are shown:
            </p>
            <ul>
              <li>🔵 Your hole cards (marked with "H")</li>
              <li>🟠 Community cards (marked with "C")</li>
              <li>❓ Hidden cards show as "?" (cannot see unused cards)</li>
            </ul>
            <p>
              After the game ends, the <strong>full 52-card deck</strong> becomes visible
              for complete audit.
            </p>
          </div>

          <div className="guide-section">
            <h4>Step 5: Independent Verification (Optional)</h4>
            <p>
              For maximum transparency, you can:
            </p>
            <ul>
              <li>📥 <strong>Export Proof</strong> - Download JSON with all RNG data</li>
              <li>🔍 <strong>Export Script</strong> - Download verification script</li>
              <li>🔗 Copy random bytes and hash for external verification</li>
            </ul>
            <p>
              Use these files to verify the shuffle on your own computer using
              any SHA-256 library. No trust required!
            </p>
          </div>

          <div className="guide-section highlight">
            <h4>✅ What Proves Transparency?</h4>
            <ul>
              <li>✅ Random bytes from IC VRF (cryptographically secure)</li>
              <li>✅ Deck hash committed BEFORE dealing (commit-reveal scheme)</li>
              <li>✅ Full deck visible after game (complete audit trail)</li>
              <li>✅ On-chain storage (immutable, cannot be deleted)</li>
              <li>✅ Independent verification possible (export and verify yourself)</li>
            </ul>
          </div>

          <div className="guide-section warning">
            <h4>⚠️ What if Verification Fails?</h4>
            <p>
              If the "Verify Shuffle" button returns <strong>❌ Failed</strong>, this means
              the deck hash does not match. This should <strong>never happen</strong> in a
              fair system. If it does:
            </p>
            <ul>
              <li>1. Export the proof data immediately</li>
              <li>2. Contact support with the proof</li>
              <li>3. Report the issue - this is a critical bug</li>
            </ul>
          </div>
        </div>
      )}
    </section>
  );
}


