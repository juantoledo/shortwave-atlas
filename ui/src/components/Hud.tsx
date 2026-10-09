// Over the globe: what the dots mean (top left) and how to use it (bottom left, fading
// once you have touched the globe).

import { COARSE, useMedia } from '../hooks/useMedia';
import { useT } from '../i18n';

export function Hud({ faded }: { faded: boolean }) {
  const t = useT();
  const touch = useMedia(COARSE);
  return (
    <>
      <ul className="hud hud-tl legend">
        <li><i className="lg on" />{t.legendOnAir}</li>
        <li><i className="lg quiet" />{t.legendQuiet}</li>
        <li><i className="lg qth" />{t.legendQth}</li>
        <li className="lg-term">{t.liveTerminator}</li>
      </ul>
      <div className={'hud hud-bl' + (faded ? ' faded' : '')}>
        {touch ? t.dragToRotateTouch : t.dragToRotate}<br />
        {touch ? t.tapForQth : t.clickForQth}<br />
        {touch ? t.tapSite : t.clickSite}
      </div>
    </>
  );
}
