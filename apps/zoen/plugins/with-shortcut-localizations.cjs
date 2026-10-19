const fs = require('node:fs/promises');
const path = require('node:path');
const { createRunOncePlugin, withXcodeProject, IOSConfig } = require('expo/config-plugins');

// AppShortcuts is a separate Apple string table; Expo config.locales generates Localizable only.
async function applyShortcutLocalizations(config) {
  const { projectRoot, platformProjectRoot, projectName } = config.modRequest;
  if (!projectName) throw new Error('Shortcut localization requires the main app target.');
  for (const locale of ['en', 'pt-BR']) {
    const source = path.join(projectRoot, 'config', 'shortcuts', `${locale}.json`);
    const table = JSON.parse(await fs.readFile(source, 'utf8'));
    const lines = Object.entries(table).map(([key, value]) => {
      if (typeof value !== 'string') throw new Error('Shortcut translations must be strings.');
      if (
        (key.match(/\$\{applicationName\}/g) ?? []).length !== 1 ||
        (value.match(/\$\{applicationName\}/g) ?? []).length !== 1
      ) {
        throw new Error('Each shortcut phrase must retain exactly one application name token.');
      }
      return `${JSON.stringify(key)} = ${JSON.stringify(value)};`;
    });
    const groupName = `${projectName}/Supporting/${locale}.lproj`;
    const directory = path.join(platformProjectRoot, groupName);
    await fs.mkdir(directory, { recursive: true });
    await fs.writeFile(path.join(directory, 'AppShortcuts.strings'), `${lines.join('\n')}\n`);
    const group = IOSConfig.XcodeUtils.ensureGroupRecursively(config.modResults, groupName);
    if (!group.children.some(({ comment }) => comment === 'AppShortcuts.strings')) {
      config.modResults = IOSConfig.XcodeUtils.addResourceFileToGroup({
        filepath: `${locale}.lproj/AppShortcuts.strings`,
        groupName,
        project: config.modResults,
        isBuildFile: true,
        verbose: false,
      });
    }
  }
  return config;
}
function withShortcutLocalizations(config) {
  return withXcodeProject(config, applyShortcutLocalizations);
}
module.exports = createRunOncePlugin(
  withShortcutLocalizations,
  'zoen-shortcut-localizations',
  '1.0.0',
);
module.exports.applyShortcutLocalizations = applyShortcutLocalizations;
