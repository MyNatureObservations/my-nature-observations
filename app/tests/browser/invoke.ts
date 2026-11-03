// Synthetic reader bridge used only by the standalone fixture's Vite config.
export async function invoke(command: string, args: { assetId?: string } = {}) {
  if (command === "get_journal_photograph") return (window as any).__photoFixtureAssets.get(args.assetId);
  if (command === "new_journal_session") return { generation: 1 };
  throw new Error(`Unexpected fixture command: ${command}`);
}
