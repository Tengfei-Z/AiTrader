const RocketBadge = () => (
  <svg
    className="rocket-badge"
    width="28"
    height="28"
    viewBox="0 0 64 64"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
  >
    <defs>
      <linearGradient id="rocket_body" x1="12" y1="6" x2="52" y2="46" gradientUnits="userSpaceOnUse">
        <stop stopColor="#6366F1" />
        <stop offset="1" stopColor="#06B6D4" />
      </linearGradient>
      <linearGradient id="rocket_flame" x1="32" y1="40" x2="32" y2="64" gradientUnits="userSpaceOnUse">
        <stop stopColor="#FDE047" />
        <stop offset="1" stopColor="#F97316" />
      </linearGradient>
    </defs>
    <path
      d="M32 6c-8 8-18 24-18 36 0 10 4 16 14 20h8c10-4 14-10 14-20 0-12-10-28-18-36z"
      fill="url(#rocket_body)"
      stroke="#0F172A"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
    <circle cx="36" cy="28" r="6" fill="#E0F2FE" stroke="#0F172A" strokeWidth="2" />
    <path
      d="M26 36l-8 10c-2 3-1 6 3 5l10-4"
      fill="#0EA5E9"
      stroke="#0F172A"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
    <path
      d="M38 36l8 10c2 3 1 6-3 5l-10-4"
      fill="#2563EB"
      stroke="#0F172A"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
    <path
      d="M32 52l-6 6-2-10 8-6 8 6-2 10z"
      fill="url(#rocket_flame)"
      stroke="#0F172A"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export default RocketBadge;
