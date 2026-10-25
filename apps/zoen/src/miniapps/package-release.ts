import type { Command } from '../core/types.ts';
import type { AppDocumentDto } from '../core/generated/AppDocumentDto.ts';
import type { AppResourceRequest } from './canonical-resource.ts';
import type { AppPackageSource } from './package-intent.ts';
import { t } from '../i18n/index.ts';

type Update = Extract<Command, { op: 'app_resource_update' }>['input'];

/** A package update never takes document bytes from the picker or its guest UI. */
export class AppPackageRelease {
  private input: Update | null = null;
  private base: AppDocumentDto | null = null;
  private task: Promise<AppDocumentDto> | null = null;
  private active = true;
  private readonly source: AppPackageSource;
  private readonly itemId: string;
  private readonly resourceUri: string;
  private readonly request: AppResourceRequest;
  private readonly current: () => boolean;
  constructor(
    source: AppPackageSource,
    itemId: string,
    resourceUri: string,
    request: AppResourceRequest,
    current: () => boolean,
  ) {
    this.source = Object.freeze({ ...source });
    this.itemId = itemId;
    this.resourceUri = resourceUri;
    this.request = request;
    this.current = current;
  }
  private check() {
    if (!this.active || !this.current())
      throw new Error(t('miniapps.theAccountHasChangedOpenTheMiniappAgain'));
  }
  update(): Promise<AppDocumentDto> {
    this.check();
    if (this.task) return this.task;
    const task = (async () => {
      if (!this.input) {
        const base = await this.request({
          op: 'app_document_read_pinned',
          item_id: this.itemId,
          expected_resource_uri: this.resourceUri,
        });
        this.check();
        const desired = `ui://zoen/package/${this.source.package_item_id}/${this.source.package_version}/${this.source.package_sha256}`;
        if (this.resourceUri === desired) return base;
        const mutation = await this.request({ op: 'new_mutation_id' });
        this.check();
        this.base = base;
        this.input = Object.freeze({
          admission: Object.freeze({ ...this.source, mutation_id: mutation }),
          item_id: this.itemId,
          expected_version: base.version,
          expected_sha256: base.sha256,
          expected_resource_uri: this.resourceUri,
        });
      }
      this.check();
      const accepted = await this.request({ op: 'app_resource_update', input: this.input });
      this.check();
      const base = this.base;
      if (
        !base ||
        accepted.item_id !== this.itemId ||
        accepted.scene_json !== base.scene_json ||
        accepted.sha256 !== base.sha256
      )
        throw new Error(t('miniapps.theMiniappCouldNotBeOpened'));
      return accepted;
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
