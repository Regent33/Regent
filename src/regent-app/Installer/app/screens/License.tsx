import { Button } from "@/app/ui/Button";
import { PageHeader } from "@/app/ui/Logo";

const MIT = `MIT License

Copyright (c) 2026 Regent33 / Rainer Lacanlale

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;

export function License({
  onBack,
  onNext,
}: {
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <PageHeader
        title="License"
        subtitle="Regent is free and open source under the MIT License."
      />
      <div
        tabIndex={0}
        aria-label="MIT License text"
        className="mt-5 flex-1 select-text overflow-y-auto whitespace-pre-wrap rounded-xl border border-stroke-tertiary bg-surface p-4 text-xs leading-relaxed text-text-secondary"
      >
        {MIT}
      </div>
      <div className="mt-4 flex items-center justify-between">
        <Button variant="ghost" onClick={onBack}>
          Back
        </Button>
        <Button onClick={onNext}>Continue</Button>
      </div>
    </div>
  );
}
