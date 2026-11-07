import type { ExecOutcome } from "../api";

type Output = Extract<ExecOutcome, { type: "output" }>;

export function OutputView({ output }: { output: Output }) {
  const ok = output.code === 0 && !output.timedOut;
  const status = output.timedOut ? "Прервано по таймауту" : output.code === null ? "Завершено сигналом" : `Код выхода ${output.code}`;
  const empty = !output.stdout.trim() && !output.stderr.trim();

  return (
    <div className="output" role="region" aria-label="Вывод команды">
      <div className="output-head">
        <span className="output-title mono">{output.title}</span>
        <span className={`badge ${ok ? "ok" : "bad"}`}>{status}</span>
      </div>
      <div className="output-body">
        {empty && <div className="output-empty">Команда ничего не вывела</div>}
        {output.stdout && <pre className="mono">{output.stdout}</pre>}
        {output.stderr && <pre className="mono stderr">{output.stderr}</pre>}
      </div>
    </div>
  );
}
