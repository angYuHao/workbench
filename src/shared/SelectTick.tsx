export default function SelectTick({ selected }: { selected: boolean }) {
  return (
    <div className={selected ? "select-tick on" : "select-tick"}>
      {selected && (
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path
            d="M2 6l3 3 5-6"
            stroke="#fff"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      )}
    </div>
  );
}
