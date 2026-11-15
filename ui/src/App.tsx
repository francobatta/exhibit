import { useEffect, useState } from "react";
import { boot, jobStatus, onJobStatus, onLibraryChanged, status } from "./lib/api";
import type { JobStatus, LibraryStatus } from "./lib/api";

// Placeholder until the real screens exist: the library status as JSON.
export default function App() {
  const [lib, setLib] = useState<LibraryStatus | null>(null);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fail = (e: unknown) => setError(JSON.stringify(e));
    boot()
      .then((b) => setLib(b.status))
      .catch(fail);
    jobStatus().then(setJob).catch(fail);
    const off = [
      onLibraryChanged(() => status().then(setLib).catch(fail)),
      onJobStatus(setJob),
    ];
    return () => off.forEach((p) => p.then((f) => f()));
  }, []);

  return (
    <main style={{ padding: "2rem", fontFamily: "Consolas, monospace", fontSize: "1rem" }}>
      <h1 style={{ fontFamily: "system-ui", fontWeight: 650 }}>Exhibit</h1>
      {error !== null && <pre style={{ color: "#e8a598" }}>{error}</pre>}
      <pre>{JSON.stringify({ job, status: lib }, null, 2)}</pre>
    </main>
  );
}
