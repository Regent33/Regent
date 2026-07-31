export const chat = {
  chat: {
    composer: {
      placeholder: 'Send follow-up',
      attach: 'Attach',
      attachRemove: 'Remove attachment',
      attachTooBig: 'File exceeds the 20 MB limit',
      dropHint: 'Drop to attach',
      mic: 'Voice input',
      micStarting: 'Starting voice input',
      micStop: 'Stop and transcribe',
      micTranscribing: 'Transcribing voice input',
      micError: 'Voice input error',
      send: 'Send',
      stop: 'Stop',
      // Shown where "core: interrupted" would sit in red — but ONLY when the
      // interruption was a barge-in (you typed over the answer). Pressing Stop
      // still reports the backend's reason; you asked for nothing to follow, so
      // an acknowledgement of a message you never sent would be a lie.
      // Rotated rather than fixed, so a conversation with several barge-ins
      // doesn't repeat one line back at you.
      interrupted: [
        'Took note — picking up your new message.',
        'Understood — switching to what you just said.',
        'Noted. Moving to your new message.',
        'Got it — dropping that and taking this instead.',
        'Right — following your new direction.',
      ],
      model: 'Model',
      openModelPicker: 'Choose a model',
      searchModels: 'Search models…',
      noModels: 'No matching models',
      fallbackActive: 'Primary model unavailable — answering on',
      slashCommands: 'Commands',
      closeCommands: 'Close commands',
      scrollToBottom: 'Scroll to bottom',
      queued: 'queued — sends when this reply finishes',
      contextOnHint: 'Regent can see this — click to stop sharing it',
      contextOffHint: 'Not shared — click to let Regent see it again',
      contextSelection: 'selection',
      contextClear: 'Forget this file',
    },
    transcript: {
      thinking: 'Thinking',
      codePlanning: 'Code task — planning (read-only exploration)',
      codeExecuting: 'Code task — executing the plan',
      verifyPassed: 'Verify passed',
      verifyFailed: 'Verify failed',
      codeReverted: 'Changes reverted to the pre-task snapshot',
      // States other than "finished" are reported as themselves — a job that
      // timed out or was cancelled must never read as done.
      jobFinished: (label: string, state: string): string =>
        state === 'finished'
          ? `Background job finished: ${label} — send a message for the details`
          : `Background job ${state.replace('_', ' ')}: ${label}`,
      approvalTitle: 'Approval needed',
      approve: 'Approve',
      deny: 'Deny',
      approved: 'Approved',
      denied: 'Denied',
    },
    markdown: {
      copyCode: 'Copy code',
      copied: 'Copied',
      expand: 'Expand',
      collapse: 'Collapse',
      openImage: 'Open image',
      closeImage: 'Close image',
      openDiagram: 'Open diagram full screen',
      closeDiagram: 'Close diagram',
      resetView: 'Reset view',
      embedLoad: 'Load',
      embedOpenExternal: 'Open externally',
      diagramError: 'Could not render diagram',
    },
  },
} as const;
