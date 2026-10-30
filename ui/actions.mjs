export function createActions(invoke, update) {
  let busy = false;
  async function run(command, request, message, success) {
    if (busy) return;
    busy = true;
    update({ busy: true, message, error: null });
    try {
      await invoke(command, { request });
      update({ busy: false, message: success, error: null });
    } catch (error) {
      update({ busy: false, message: '', error });
    } finally {
      busy = false;
    }
  }
  return {
    connect: request => run('launch_workspace', request, 'Connecting to Hermes…', 'Workspace opened. Your gateway keeps running when you close this app.'),
    build: request => run('build_renderer', request, 'Building the local workspace. This can take several minutes…', 'Local workspace built. Choose Open workspace to connect.'),
  };
}
