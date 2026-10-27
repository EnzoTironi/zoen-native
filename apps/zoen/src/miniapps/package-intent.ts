import type { Command, ItemDetail } from '../core/types.ts';
import type { AppResourceRequest } from './canonical-resource.ts';
import { t } from '../i18n/index.ts';
import { isOwnedAppResource } from './canonical-resource.ts';

type Install = Extract<Command, { op: 'app_resource_install' }>['input'];
export type AppPackageSource = Readonly<Omit<Install, 'mutation_id'>>;

export function isAppPackage(item: ItemDetail): boolean {
  return item.kind_id === 'file' && item.file?.mime === 'text/html;profile=mcp-app';
}

/** Only presentation lifetime state. Core's signed history resolves cold/lost-ACK recovery. */
export class AppPackageIntent {
  private input: Install | null = null;
  private task: Promise<ItemDetail> | null = null;
  private active = true;
  private readonly source: AppPackageSource;
  private readonly request: AppResourceRequest;
  private readonly current: () => boolean;
  constructor(source: AppPackageSource, request: AppResourceRequest, current: () => boolean) {
    this.source = Object.freeze({ ...source });
    this.request = request;
    this.current = current;
  }
  private check() {
    if (!this.active || !this.current())
      throw new Error(t('miniapps.theAccountHasChangedOpenTheMiniappAgain'));
  }
  open(): Promise<ItemDetail> {
    this.check();
    if (this.task) return this.task;
    const task = (async () => {
      if (!this.input) {
        const mutation = await this.request({ op: 'new_mutation_id' });
        this.check();
        this.input = Object.freeze({ ...this.source, mutation_id: mutation });
      }
      this.check();
      // A refusal/ambiguous lookup is never converted into another installation.
      const found = await this.request({ op: 'app_resource_find', input: this.input });
      this.check();
      const item = found ?? (await this.request({ op: 'app_resource_install', input: this.input }));
      this.check();
      // An already accepted creation may now have a newer release. Core returns
      // the same canonical Item; never reject it and generate a second intent.
      if (
        item.space_id !== this.source.space_id ||
        !item.app ||
        !isOwnedAppResource(item.app.resource_uri)
      )
        throw new Error(t('miniapps.thisResourceDoesNotBelongToTheOpen'));
      return item;
    })();
    this.task = task;
    void task
      .finally(() => {
        if (this.task === task) this.task = null;
      })
      .catch(() => {});
    return task;
  }
  close() {
    this.active = false;
  }
}
