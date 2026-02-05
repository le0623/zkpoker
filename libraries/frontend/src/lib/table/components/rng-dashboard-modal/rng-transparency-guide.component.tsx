import classNames from 'classnames';
import { memo, useState } from 'react';

import { Interactable } from '@zk-game-dao/ui';

export const RngTransparencyGuideComponent = memo(() => {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="material rounded-lg p-4 mb-4">
      <Interactable
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex items-center justify-between cursor-pointer"
      >
        <h3 className="type-top">📖 How to Verify Transparency</h3>
        <span className="text-xl">{isExpanded ? '▼' : '▶'}</span>
      </Interactable>

      {isExpanded && (
        <div className="mt-4 space-y-4">
          <div>
            <h4 className="type-button-2 mb-2">Step 1: Check the Randomness Source</h4>
            <p className="type-tiny opacity-70">
              Verify that the random bytes come from <strong>Internet Computer VRF</strong>.
              This is the same cryptographic randomness used by the blockchain - it cannot be
              predicted or manipulated by anyone, including the house.
            </p>
          </div>

          <div>
            <h4 className="type-button-2 mb-2">Step 2: Verify the Deck Hash</h4>
            <p className="type-tiny opacity-70">
              The <strong>Deck Hash</strong> was calculated and stored <strong>BEFORE</strong> any
              cards were dealt. This means:
            </p>
            <ul className="list-disc list-inside space-y-1 type-tiny opacity-70">
              <li>✅ The house cannot change cards after seeing what's dealt</li>
              <li>✅ The deck order is cryptographically committed</li>
              <li>✅ Any tampering would produce a different hash</li>
            </ul>
          </div>

          <div>
            <h4 className="type-button-2 mb-2">Step 3: Click "Verify Shuffle" Button (After Game Ends)</h4>
            <p className="type-tiny opacity-70">
              <strong>🔒 Security Note:</strong> The "Verify Shuffle" button is only available
              <strong> after the game ends</strong>. This ensures the commit-reveal security model:
              the deck hash is committed before dealing, and verification data is only revealed
              after the game concludes.
            </p>
            <p className="type-tiny opacity-70">
              Once enabled, this button re-computes the deck hash using the same random bytes
              and algorithm. If the hash matches the stored hash, the shuffle is proven fair.
            </p>
          </div>

          <div>
            <h4 className="type-button-2 mb-2">Step 4: Check Revealed Cards (During Game)</h4>
            <p className="type-tiny opacity-70">
              During gameplay, only revealed cards are shown:
            </p>
            <ul className="list-disc list-inside space-y-1 type-tiny opacity-70">
              <li>🔵 Your hole cards (marked with "H")</li>
              <li>🟠 Community cards (marked with "C")</li>
              <li>❓ Hidden cards show as "?" (cannot see unused cards)</li>
            </ul>
            <p className="type-tiny opacity-70">
              After the game ends, the <strong>full 52-card deck</strong> becomes visible
              for complete audit.
            </p>
          </div>

          <div>
            <h4 className="type-button-2 mb-2">Step 5: Independent Verification (After Game Ends)</h4>
            <p className="type-tiny opacity-70">
              <strong>🔒 Security Note:</strong> Export functions are only available
              <strong> after the game ends</strong> to protect sensitive data during gameplay.
            </p>
            <p className="type-tiny opacity-70">
              For maximum transparency, you can:
            </p>
            <ul className="list-disc list-inside space-y-1 type-tiny opacity-70">
              <li>📥 <strong>Export Proof</strong> - Download JSON with all RNG data (available after game ends)</li>
              <li>🔍 <strong>Export Script</strong> - Download verification script (available after game ends)</li>
              <li>🔗 Copy random bytes and hash for external verification (visible from start)</li>
            </ul>
            <p className="type-tiny opacity-70">
              Use these files to verify the shuffle on your own computer using
              any SHA-256 library. No trust required!
            </p>
          </div>

          <div className="material rounded p-3 bg-green-500/10 border border-green-500/30">
            <h4 className="type-button-2 mb-2">✅ What Proves Transparency?</h4>
            <ul className="list-disc list-inside space-y-1 type-tiny opacity-70">
              <li>✅ Random bytes from IC VRF (cryptographically secure, visible from start)</li>
              <li>✅ Deck hash committed BEFORE dealing (commit-reveal scheme)</li>
              <li>✅ Verification only after game ends (prevents deck reconstruction during play)</li>
              <li>✅ Full deck visible after game (complete audit trail)</li>
              <li>✅ On-chain storage (immutable, cannot be deleted)</li>
              <li>✅ Independent verification possible (export and verify yourself after game ends)</li>
            </ul>
          </div>

          <div className="material rounded p-3 bg-yellow-500/10 border border-yellow-500/30">
            <h4 className="type-button-2 mb-2">⚠️ What if Verification Fails?</h4>
            <p className="type-tiny opacity-70">
              If the "Verify Shuffle" button returns <strong>❌ Failed</strong>, this means
              the deck hash does not match. This should <strong>never happen</strong> in a
              fair system. If it does:
            </p>
            <ul className="list-decimal list-inside space-y-1 type-tiny opacity-70">
              <li>Export the proof data immediately</li>
              <li>Contact support with the proof</li>
              <li>Report the issue - this is a critical bug</li>
            </ul>
          </div>
        </div>
      )}
    </div>
  );
});
RngTransparencyGuideComponent.displayName = 'RngTransparencyGuideComponent';


