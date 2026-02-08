import { useEffect, useState } from "react";
import type { SentimentsOutput } from "../types";
import { fetchSentiments } from "../api";

export function SentimentsPage() {
  const [data, setData] = useState<SentimentsOutput | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchSentiments()
      .then((d) => {
        setData(d);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setLoading(false);
      });
  }, []);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-zinc-500">
        Loading sentiments...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-red-400">
        {error}
      </div>
    );
  }

  if (!data) return null;

  const hasSentiments = data.sentiments.length > 0;

  return (
    <div className="h-full overflow-auto p-6">
      <div className="max-w-3xl mx-auto space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-zinc-100">Sentiments</h2>
          <p className="text-sm text-zinc-500 mt-1">
            Agent intuitions recorded across all annotations in this repository.
          </p>
        </div>

        {!hasSentiments && (
          <p className="text-sm text-zinc-500">No sentiments recorded yet.</p>
        )}

        {hasSentiments && (
          <div className="space-y-3">
            {data.sentiments.map((s, i) => (
              <div
                key={i}
                className="rounded border border-zinc-800 bg-zinc-900/50 p-4"
              >
                <div className="flex items-start justify-between gap-3">
                  <p className="text-sm text-zinc-200">{s.detail}</p>
                  <FeelingBadge feeling={s.feeling} />
                </div>
                <p className="text-xs text-zinc-500 mt-2">{s.summary}</p>
                <div className="flex gap-2 mt-2 text-xs text-zinc-600">
                  <code>{s.commit.slice(0, 7)}</code>
                  <span>{s.timestamp}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function FeelingBadge({ feeling }: { feeling: string }) {
  const lower = feeling.toLowerCase();
  let color = "bg-zinc-800 text-zinc-400";

  if (lower === "worry") {
    color = "bg-amber-900/60 text-amber-400";
  } else if (lower === "confidence") {
    color = "bg-green-900/60 text-green-400";
  } else if (lower === "uncertainty") {
    color = "bg-blue-900/60 text-blue-400";
  } else if (lower === "frustration") {
    color = "bg-red-900/60 text-red-400";
  }

  return (
    <span className={`shrink-0 rounded px-1.5 py-0.5 text-xs ${color}`}>
      {feeling}
    </span>
  );
}
