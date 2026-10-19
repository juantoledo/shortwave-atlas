// The button that folds a side panel to its rail or a faceplate cell to its first line
// (hooks/folds.ts). One chevron, turned (styles.css) to point the way the thing will go: toward
// the edge it folds against while open, back out while folded.

interface Props {
  /** The edge it folds against: a side panel's column side, or the faceplate's bottom. */
  edge: 'left' | 'right' | 'bottom';
  expanded: boolean;
  label: string;
  onClick: () => void;
  /** The id of what it folds. */
  controls?: string;
  className?: string;
}

export function FoldButton({ edge, expanded, label, onClick, controls, className }: Props) {
  return (
    <button
      className={'fold-btn' + (className ? ' ' + className : '')}
      type="button"
      data-edge={edge}
      aria-expanded={expanded}
      aria-controls={controls}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <svg className="fold-chev" viewBox="0 0 12 8" aria-hidden="true"><path d="M1.5 1.5 6 6l4.5-4.5" /></svg>
    </button>
  );
}
