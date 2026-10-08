import { useNow } from '../hooks/useCore';
import { useT } from '../i18n';

const two = (n: number) => String(n).padStart(2, '0');

export function Hud({ frame, onFrame }: { frame: boolean; onFrame: () => void }) {
  const t = useT();
  const d = useNow(1000);
  return (
    <>
      <div className="hud hud-tl">
        UTC <b>{two(d.getUTCHours())}:{two(d.getUTCMinutes())}:{two(d.getUTCSeconds())}</b><br />{t.liveTerminator}
      </div>
      <div className="hud hud-bl">{t.dragToRotate}<br />{t.clickForQth}</div>
      <div className="hud hud-br">
        <button className="toggle" type="button" aria-pressed={frame} onClick={onFrame}>{t.frameRoute}</button>
      </div>
    </>
  );
}
