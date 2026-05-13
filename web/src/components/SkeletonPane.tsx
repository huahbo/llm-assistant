interface Props {
  rows?: number;
  header?: boolean;
}

export default function SkeletonPane({ rows = 6, header = true }: Props) {
  return (
    <div className="skeleton-pane">
      {header && <div className="skeleton-pane__header" />}
      <div className="skeleton-pane__body">
        {Array.from({ length: rows }).map((_, i) => (
          <div
            key={i}
            className="skeleton-pane__row"
            style={{ width: `${65 + (i % 3) * 12}%` }}
          />
        ))}
      </div>
    </div>
  );
}
