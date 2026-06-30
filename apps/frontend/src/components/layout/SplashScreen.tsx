/**
 * SplashScreen — centered startup overlay shown while the IDE initializes.
 *
 * Displays the AppLogo, app name, and an animated loading indicator.
 * Fades out when `visible` becomes false.
 */

import { useEffect, useState } from 'react';
import { AppLogo } from './AppLogo';

interface Props {
  visible: boolean;
}

const LOADING_STEPS = [
  'Initializing agent runtime…',
  'Loading workspace…',
  'Establishing connections…',
  'Almost ready…',
];

export function SplashScreen({ visible }: Props) {
  const [step, setStep] = useState(0);
  const [fadeOut, setFadeOut] = useState(false);

  // Cycle through loading messages.
  useEffect(() => {
    if (!visible) return;
    const timer = setInterval(() => {
      setStep((s) => (s + 1) % LOADING_STEPS.length);
    }, 1200);
    return () => clearInterval(timer);
  }, [visible]);

  // When visibility changes to false, trigger fade-out before unmount.
  useEffect(() => {
    if (!visible) {
      setFadeOut(true);
    }
  }, [visible]);

  if (!visible && fadeOut) {
    return (
      <div className="fixed inset-0 z-[100] flex items-center justify-center bg-bg-primary transition-opacity duration-500 opacity-0 pointer-events-none">
        <div className="flex flex-col items-center gap-6">
          <AppLogo size={80} className="text-accent" />
          <p className="text-sm font-semibold text-text-primary tracking-wide">
            Remote AI IDE
          </p>
        </div>
      </div>
    );
  }

  if (!visible) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-bg-primary">
      <div className="flex flex-col items-center gap-6">
        {/* Logo with subtle pulse */}
        <div className="animate-pulse">
          <AppLogo size={80} className="text-accent" />
        </div>

        {/* App name */}
        <div className="flex flex-col items-center gap-2">
          <h1 className="text-base font-bold text-text-primary tracking-wider">
            Remote AI IDE
          </h1>
          <div className="w-16 h-px bg-border" />
        </div>

        {/* Loading indicator */}
        <div className="flex flex-col items-center gap-2.5">
          <div className="flex gap-1">
            {[0, 1, 2].map((i) => (
              <div
                key={i}
                className="w-1.5 h-1.5 rounded-full bg-accent/70 animate-bounce"
                style={{ animationDelay: `${i * 150}ms` }}
              />
            ))}
          </div>
          <p className="text-xs text-text-secondary min-w-[180px] text-center transition-opacity duration-300">
            {LOADING_STEPS[step]}
          </p>
        </div>
      </div>
    </div>
  );
}
