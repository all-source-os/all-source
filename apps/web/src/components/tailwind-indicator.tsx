import Image from "next/image";

export function TailwindIndicator() {
  // Don't show in production
  if (process.env.NODE_ENV === "production") return null;
  return (
    <div className="fixed bottom-12 left-3 z-50 flex h-8 w-8 items-center justify-center rounded-full bg-background/80 backdrop-blur-sm border border-border shadow-lg p-1">
      <Image src="/logo.svg" alt="" width="24" height="24" className="h-6 w-6 object-contain" />
    </div>
  );
}
