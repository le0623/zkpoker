import classNames from 'classnames';
import { memo } from 'react';

import { ButtonComponent } from '@zk-game-dao/ui';

import type { RngMetadata } from '../../types/rng.types';

type VerificationStatus = 'idle' | 'pending' | 'verified' | 'failed';

export const RngVerificationPanelComponent = memo<{
  verificationStatus: VerificationStatus;
  onVerify: () => void;
  rngData?: RngMetadata;
  gameEnded?: boolean;
}>(({ verificationStatus, onVerify, gameEnded = false }) => {
  return (
    <div className="material bg-transparent rounded-lg p-4 mb-4">
      <h3 className="type-top mb-4">✅ Verification</h3>

      <div className="space-y-4">
        <div>
          {verificationStatus === 'verified' && (
            <div className="material rounded p-4">
              <div className="flex items-center gap-2">
                <span className="text-2xl">✅</span>
                <div>
                  <div className="type-button-2 text-green-500">Shuffle Verified!</div>
                  <div className="type-tiny opacity-70">
                    The deck hash matches. This shuffle was fair and has not been
                    tampered with.
                  </div>
                </div>
              </div>
            </div>
          )}

          {verificationStatus === 'failed' && (
            <div className="material rounded p-4">
              <div className="flex items-center gap-2">
                <span className="text-2xl">❌</span>
                <div>
                  <div className="type-button-2 text-red-500">Verification Failed</div>
                  <div className="type-tiny opacity-70">
                    The deck hash does not match. Please contact support.
                  </div>
                </div>
              </div>
            </div>
          )}

          {verificationStatus === 'pending' && (
            <div className="material rounded p-4">
              <div className="flex items-center gap-2">
                <span className="text-2xl">⏳</span>
                <div>
                  <div className="type-button-2 text-yellow-500">Verifying...</div>
                  <div className="type-tiny opacity-70">
                    Re-computing deck hash from random bytes...
                  </div>
                </div>
              </div>
            </div>
          )}

          {verificationStatus === 'idle' && (
            <div className="material rounded p-4">
              <div className="type-tiny opacity-70">
                {gameEnded
                  ? 'Click the button to verify that the shuffle was fair.'
                  : 'Verification will be available after the game ends.'}
              </div>
            </div>
          )}
        </div>

        <div>
          <ButtonComponent
            onClick={onVerify}
            isDisabled={verificationStatus === 'pending' || !gameEnded}
            variant="material"
            className="w-full"
            title={
              gameEnded
                ? 'Verify that the shuffle was fair'
                : 'Verification available after game ends'
            }
          >
            {verificationStatus === 'pending' ? 'Verifying...' : 'Verify Shuffle'}
          </ButtonComponent>
        </div>

        <div className="space-y-3">
          <p className="type-subheadline">
            <strong>How verification works:</strong>
          </p>
          <p className="type-tiny opacity-70">
            <strong>🔒 Note:</strong> Verification is only available after the
            game ends to ensure the commit-reveal security model. During
            gameplay, only the deck hash and random bytes are visible.
          </p>
          <ol className="list-decimal list-inside space-y-1 type-tiny opacity-70">
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

        <div className="space-y-3">
          <p className="type-subheadline">
            <strong>🔒 Hash-Based Verification:</strong>
          </p>
          <ul className="list-disc list-inside space-y-1 type-tiny opacity-70">
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
    </div>
  );
});
RngVerificationPanelComponent.displayName = 'RngVerificationPanelComponent';
