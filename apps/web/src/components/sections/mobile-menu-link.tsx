"use client";

import Link from "next/link";

export default function MobileMenuLink({
  href,
  label,
  className,
}: {
  href: string;
  label: string;
  className: string;
}) {
  return (
    <Link
      href={href}
      className={className}
      onClick={(event) => event.currentTarget.closest("details")?.removeAttribute("open")}
    >
      {label}
    </Link>
  );
}
