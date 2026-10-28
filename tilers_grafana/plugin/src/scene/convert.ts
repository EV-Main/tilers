// A dashboard save model as an `EmbeddedScene` laid out by `DockLayout`. The spec is Grafana's own
// `transformSaveModelToScene.ts`; whatever the dock does not render throws instead of being dropped.

import { DataTransformerConfig, FieldConfigSource, PanelMenuItem } from '@grafana/data';
import { config, locationService } from '@grafana/runtime';
import { Dashboard, DashboardLink, GridPos, Panel, RowPanel, VariableModel } from '@grafana/schema';
import {
  behaviors,
  CustomVariable,
  DataSourceVariable,
  EmbeddedScene,
  getExploreURL,
  QueryVariable,
  SceneControlsSpacer,
  SceneDataTransformer,
  sceneGraph,
  SceneQueryRunner,
  SceneReactObject,
  SceneRefreshPicker,
  SceneTimePicker,
  SceneTimeRange,
  sceneUtils,
  SceneVariable,
  SceneVariableSet,
  TextBoxVariable,
  VariableValueSelectors,
  VizPanel,
  VizPanelMenu,
} from '@grafana/scenes';
import { isWeekStart } from '@grafana/ui';

import { DashboardLinks, NativeLink } from './controls';
import { DockLayout, DockSpec } from './DockLayout';
import { PanelTimeRange } from './PanelTimeRange';

type Reject = (where: string, feature: string) => never;

/** A dashboard feature the dock does not render; refused rather than dropped. */
export class Unsupported extends Error {}

export function toScene(dash: Dashboard & { uid: string }): EmbeddedScene {
  const reject: Reject = (where, feature) => {
    throw new Unsupported(`Dashboard "${dash.uid}": ${where} uses ${feature}, which the dock does not render.`);
  };
  for (const a of dash.annotations?.list ?? []) {
    if (!a.builtIn) {
      reject(`annotation "${a.name}"`, 'a non-built-in annotation query');
    }
  }
  if (dash.weekStart && !isWeekStart(dash.weekStart)) {
    reject('the time range', `weekStart "${dash.weekStart}"`);
  }
  const links = (dash.links ?? []).map((l) => checkLink(l, reject));
  const panels = (dash.panels ?? []).map((p) => toPanel(dash.uid, p, reject));
  // Grafana's own kiosk flags, so an embedder hides the same controls here as on `/d/`
  const url = locationService.getSearchObject();
  const hideTime = dash.timepicker?.hidden || '_dash.hideTimePicker' in url;
  const hideVariables = '_dash.hideVariables' in url;
  const hideLinks = '_dash.hideLinks' in url;
  const host = '_tilers.host' in url;
  if (host && window.parent === window) {
    throw new Error('`_tilers.host` says an embedding page answers, but this page is not framed');
  }
  const refresh = new SceneRefreshPicker({ refresh: dash.refresh, intervals: dash.timepicker?.refresh_intervals, withText: true });
  const controls = [
    ...(hideVariables ? [] : [new VariableValueSelectors({})]),
    new SceneControlsSpacer(),
    ...(links.length && !hideLinks ? [new SceneReactObject({ component: DashboardLinks, props: { uid: dash.uid, links } })] : []),
    ...(hideTime ? [] : [new SceneTimePicker({ quickRanges: dash.timepicker?.quick_ranges }), refresh]),
    ...(hideLinks ? [] : [new SceneReactObject({ component: NativeLink, props: { uid: dash.uid } })]),
  ];

  return new EmbeddedScene({
    $timeRange: new SceneTimeRange({
      from: dash.time?.from,
      to: dash.time?.to,
      timeZone: dash.timezone,
      weekStart: dash.weekStart && isWeekStart(dash.weekStart) ? dash.weekStart : undefined,
      fiscalYearStartMonth: dash.fiscalYearStartMonth,
      UNSAFE_nowDelay: dash.timepicker?.nowDelay,
    }),
    $variables: new SceneVariableSet({ variables: (dash.templating?.list ?? []).map((v) => toVariable(v, reject)) }),
    $behaviors: [
      new behaviors.CursorSync({ sync: dash.graphTooltip }),
      new behaviors.LiveNowTimer({ enabled: Boolean(dash.liveNow) }),
      ...(hideTime ? [refresh] : []), // off screen it still ticks `dash.refresh`
    ],
    controls: controls.length > 1 ? controls : undefined, // a lone spacer would still draw the controls row
    body: new DockLayout({
      children: panels.map((p) => p.panel),
      gridPos: Object.fromEntries(panels.map((p) => [p.panel.state.key!, p.gridPos])),
      dock: Object.fromEntries(panels.map((p) => [p.panel.state.key!, p.dock])),
      storageKey: `dv-grafana-${dash.uid}`,
      host,
      titles: new Map(),
    }),
  });
}

function checkLink(link: DashboardLink, reject: Reject): DashboardLink {
  const where = `dashboard link "${link.title}"`;
  if (link.type !== 'dashboards') {
    reject(where, `link type "${link.type}"`);
  }
  if (!link.asDropdown) {
    reject(where, 'inline (non-dropdown) links');
  }
  return link;
}

function toPanel(uid: string, p: Panel | RowPanel, reject: Reject): { panel: VizPanel; gridPos: GridPos; dock: DockSpec } {
  const where = `panel ${p.id} "${p.title}"`;
  if (p.type === 'row') {
    reject(where, 'a row');
  }
  const panel = p as Panel;
  if (panel.repeat) {
    reject(where, 'repeat');
  }
  if (panel.libraryPanel) {
    reject(where, 'a library panel');
  }
  if (panel.links?.length) {
    reject(where, 'panel links');
  }
  if (panel.timeCompare) {
    reject(where, 'time comparison');
  }
  if (panel.datasource?.uid === '-- Dashboard --') {
    reject(where, 'the Dashboard datasource');
  }
  if (panel.id === undefined || !panel.gridPos) {
    throw new Error(`Dashboard "${uid}": ${where} has no ${panel.id === undefined ? 'id' : 'gridPos'}.`);
  }
  const key = `panel-${panel.id}`;

  return {
    gridPos: panel.gridPos,
    dock: toDock(uid, key, where, panel),
    panel: new VizPanel({
      key,
      title: panel.title,
      description: panel.description,
      pluginId: panel.type,
      pluginVersion: panel.pluginVersion,
      options: panel.options ?? {},
      fieldConfig: (panel.fieldConfig ?? { defaults: {}, overrides: [] }) as FieldConfigSource,
      displayMode: panel.transparent ? 'transparent' : undefined,
      hoverHeader: true, // the dock tab carries the title
      seriesLimit: config.panelSeriesLimit,
      $data: toData(panel),
      $timeRange:
        panel.timeFrom || panel.timeShift
          ? new PanelTimeRange({ timeFrom: panel.timeFrom, timeShift: panel.timeShift })
          : undefined,
      menu: new VizPanelMenu({ $behaviors: [(menu: VizPanelMenu) => menu.setItems(menuItems(uid, menu.parent as VizPanel))] }),
    }),
  };
}

/**
 * The panel's `dock` key, outside Grafana's schema: `minWRem`/`minHRem` are the rem its content needs, or
 * `precompile` asks the plugin to measure them once and cache the result.
 */
function toDock(uid: string, key: string, where: string, panel: Panel & { dock?: unknown }): DockSpec {
  const dock = panel.dock ?? {};
  const bad = (what: string): never => {
    throw new Error(`Dashboard "${uid}": ${where} has ${what} in its dock key.`);
  };
  if (typeof dock !== 'object' || dock === null || Array.isArray(dock)) {
    return bad(`a non-object (${JSON.stringify(dock)})`);
  }
  const { minWRem, minHRem, precompile, ...rest } = dock as Record<string, unknown>;
  if ('minW' in rest || 'minH' in rest) {
    bad('minW/minH (px, before 0.5.0) instead of minWRem/minHRem');
  }
  if (Object.keys(rest).length) {
    bad(`unknown fields ${Object.keys(rest).join(', ')}`);
  }
  const rem = (name: string, v: unknown) => {
    if (v === undefined) {
      return 0;
    }
    if (typeof v !== 'number' || !Number.isFinite(v) || v <= 0) {
      return bad(`${name}=${JSON.stringify(v)}, not a positive rem count`);
    }
    return v;
  };
  if (precompile === undefined) {
    return { minWRem: rem('minWRem', minWRem), minHRem: rem('minHRem', minHRem) };
  }
  if (precompile !== true) {
    bad(`precompile=${JSON.stringify(precompile)}, not true`);
  }
  if (minWRem !== undefined || minHRem !== undefined) {
    bad('both precompile and an authored min');
  }
  const { gridPos, ...model } = panel;
  return { precompile: { store: `dv-grafana-min-${uid}-${key}`, model: fnv1a(JSON.stringify(model)) } };
}

function fnv1a(s: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  }
  return (h >>> 0).toString(16);
}

function toData(panel: Panel) {
  if (!panel.targets?.length || config.panels[panel.type]?.skipDataQuery) {
    return undefined;
  }
  return new SceneDataTransformer({
    $data: new SceneQueryRunner({
      datasource: panel.datasource,
      queries: panel.targets as SceneQueryRunner['state']['queries'],
      maxDataPoints: panel.maxDataPoints,
      maxDataPointsFromWidth: true,
      cacheTimeout: panel.cacheTimeout,
      queryCachingTTL: panel.queryCachingTTL,
      minInterval: panel.interval,
    }),
    transformations: (panel.transformations ?? []) as DataTransformerConfig[],
  });
}

/** View and Edit open the native dashboard, which owns both. */
function menuItems(uid: string, panel: VizPanel): PanelMenuItem[] {
  const key = panel.state.key!;
  const native = (param: string, value: string) => () => {
    const state = flat(sceneUtils.getUrlState(sceneGraph.getAncestor(panel, EmbeddedScene)));
    locationService.push(`/d/${uid}?${new URLSearchParams({ ...state, [param]: value })}`);
  };
  return [
    { text: 'View', iconClassName: 'eye', onClick: native('viewPanel', key) },
    { text: 'Edit', iconClassName: 'edit', onClick: native('editPanel', key.replace('panel-', '')) },
    {
      text: 'Explore',
      iconClassName: 'compass',
      onClick: async () => {
        const data = sceneGraph.getData(panel).state.data;
        if (!data) {
          throw new Error(`${key} has no data to explore yet`);
        }
        locationService.push(await getExploreURL(data, panel, sceneGraph.getTimeRange(panel).state.value.raw));
      },
    },
  ];
}

function flat(state: ReturnType<typeof sceneUtils.getUrlState>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(state).flatMap(([k, v]) => (v === null || v === undefined ? [] : [[k, Array.isArray(v) ? v.join(',') : v]]))
  );
}

function toVariable(v: VariableModel & { definition?: string }, reject: Reject): SceneVariable {
  const common = {
    name: v.name,
    label: v.label,
    description: v.description,
    hide: v.hide,
    skipUrlSync: v.skipUrlSync,
    value: (v.current?.value ?? '') as string | string[],
    text: (v.current?.text ?? '') as string | string[],
  };
  const all = { allValue: v.allValue || undefined, includeAll: v.includeAll, defaultToAll: Boolean(v.includeAll) };
  switch (v.type) {
    case 'query':
      return new QueryVariable({
        ...common,
        ...all,
        query: v.query as QueryVariable['state']['query'],
        datasource: v.datasource,
        sort: v.sort,
        refresh: v.refresh,
        regex: v.regex,
        isMulti: v.multi,
        definition: v.definition,
        allowCustomValue: v.allowCustomValue,
      });
    case 'textbox':
      return new TextBoxVariable({ ...common, value: (Array.isArray(common.value) ? common.value[0] : common.value) || (v.query as string) });
    case 'custom':
      return new CustomVariable({ ...common, ...all, query: v.query as string, isMulti: v.multi, allowCustomValue: v.allowCustomValue });
    case 'datasource':
      return new DataSourceVariable({
        ...common,
        ...all,
        pluginId: v.query as string,
        regex: v.regex,
        isMulti: v.multi,
        defaultOptionEnabled: v.current?.value === 'default' && v.current?.text === 'default',
        allowCustomValue: v.allowCustomValue,
      });
    default:
      return reject(`variable $${v.name}`, `a ${v.type} variable`);
  }
}
