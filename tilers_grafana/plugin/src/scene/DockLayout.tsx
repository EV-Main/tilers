import { css } from '@emotion/css';
import React, { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { GrafanaTheme2 } from '@grafana/data';
import { GridPos } from '@grafana/schema';
import { LazyLoader, SceneComponentProps, SceneObjectBase, SceneObjectState, VariableDependencyConfig, VizPanel } from '@grafana/scenes';
import { ContextMenu, Menu, useStyles2 } from '@grafana/ui';

import init, { Dock, mount, Seed, Unit } from '../wasm/tilers_grafana';
import wasm from '../wasm/tilers_grafana_bg.wasm';
import { cached, compile, Precompile } from './precompile';

const ready = init({ module_or_path: wasm });

/** What a panel's content needs, in rem (`0` = nothing beyond the chrome), or a size to compile. */
export type DockSpec = { minWRem: number; minHRem: number } | { precompile: Precompile };

interface DockLayoutState extends SceneObjectState {
  children: VizPanel[];
  /** By panel key, which is also the panel's dock id. */
  gridPos: Record<string, GridPos>;
  /** By panel key. */
  dock: Record<string, DockSpec>;
  storageKey: string;
  /** An embedding page answers with the published layouts (`?_tilers.host`); nothing mounts before it does. */
  host: boolean;
  /** Interpolated, by panel key. */
  titles: Map<string, string>;
}

const BANDS = ['sm', 'md', 'xl'];

interface HostAnswer {
  published: Map<string, string>;
  publish: boolean;
}

/** The embedding page's `host` message, which is untrusted input: whatever is off throws, naming it. */
function parseHost(m: Record<string, unknown>): HostAnswer {
  const { published, publish } = m;
  if (typeof publish !== 'boolean') {
    throw new Error(`tilers host message: "publish" must be a boolean, got ${JSON.stringify(publish)}`);
  }
  if (typeof published !== 'object' || published === null || Array.isArray(published)) {
    throw new Error(`tilers host message: "published" must be an object, got ${JSON.stringify(published)}`);
  }
  const layouts = new Map<string, string>();
  for (const [band, layout] of Object.entries(published)) {
    if (!BANDS.includes(band)) {
      throw new Error(`tilers host message: published band "${band}" is not one of ${BANDS.join(', ')}`);
    }
    if (typeof layout !== 'string') {
      throw new Error(`tilers host message: published["${band}"] must be a layout string, got ${typeof layout}`);
    }
    layouts.set(band, layout);
  }
  return { published: layouts, publish };
}

/** The dashboard body: panels laid out by the tilers grid, each rendered natively through a portal. */
export class DockLayout extends SceneObjectBase<DockLayoutState> {
  public static Component = DockLayoutRenderer;

  protected _variableDependency = new VariableDependencyConfig(this, { variableNames: [], onAnyVariableChanged: () => this.retitle() });

  public constructor(state: DockLayoutState) {
    super(state);
    this.addActivationHandler(() => this.retitle());
  }

  private retitle() {
    this.setState({ titles: new Map(this.state.children.map((p) => [p.state.key!, p.interpolate(p.state.title, undefined, 'text')])) });
  }
}

function DockLayoutRenderer({ model }: SceneComponentProps<DockLayout>) {
  const { children, gridPos, dock: specs, storageKey, host, titles } = model.useState();
  const styles = useStyles2(getStyles);
  const root = useRef<HTMLDivElement>(null);
  const pointer = useRef({ x: 0, y: 0 });
  const [dock, setDock] = useState<Dock>();
  const [slots, setSlots] = useState<ReadonlyMap<string, HTMLElement>>(new Map());
  const dockRef = useRef<Dock>();
  const [answer, setAnswer] = useState<HostAnswer>();
  const [menu, setMenu] = useState<{ gid: bigint; x: number; y: number }>();

  useEffect(() => {
    if (!host) {
      return;
    }
    let answered = false;
    const onMessage = (e: MessageEvent) => {
      const m = e.data;
      if (e.source !== window.parent || typeof m !== 'object' || m === null || !('tilers' in m)) {
        return; // not addressed to us
      }
      switch (m.tilers) {
        case 'host':
          if (answered) {
            throw new Error('tilers host message: a second `host` for one `ready`');
          }
          answered = true;
          setAnswer(parseHost(m));
          break;
        case 'notify':
          if (!answered) {
            throw new Error('tilers notify message: arrived before `host`');
          }
          if (typeof m.text !== 'string') {
            throw new Error(`tilers notify message: "text" must be a string, got ${JSON.stringify(m.text)}`);
          }
          if (!dockRef.current) {
            throw new Error('tilers notify message: no dock is mounted to show it');
          }
          dockRef.current.notify(m.text);
          break;
        default:
          throw new Error(`tilers message: unknown kind ${JSON.stringify(m.tilers)}`);
      }
    };
    window.addEventListener('message', onMessage);
    window.parent.postMessage({ tilers: 'ready' }, '*'); // the parent's origin is unknowable, and a layout is no secret
    return () => {
      window.removeEventListener('message', onMessage);
      setAnswer(undefined);
    };
  }, [host]);

  useEffect(() => {
    if (host && !answer) {
      return;
    }
    let live: Dock | undefined;
    let gone = false;
    ready.then(() => {
      if (gone) {
        return;
      }
      const seeds = children.map((p) => {
        const g = gridPos[p.state.key!];
        const spec = specs[p.state.key!];
        const [min, unit] =
          'precompile' in spec ? [cached(spec.precompile) ?? { w: 0, h: 0 }, Unit.Px] : [{ w: spec.minWRem, h: spec.minHRem }, Unit.Rem];
        return new Seed(p.state.key!, p.state.title, g.x!, g.y!, g.w, g.h, min.w, min.h, unit);
      });
      live = mount(
        root.current!,
        seeds,
        storageKey,
        answer?.published ?? new Map(),
        answer?.publish
          ? (band: string, layout: string) => window.parent.postMessage({ tilers: 'publish', band, layout }, '*')
          : undefined,
        (id: string, el: HTMLElement) => setSlots((m) => new Map(m).set(id, el)),
        (gid: bigint) => setMenu({ gid, ...pointer.current })
      );
      dockRef.current = live;
      setDock(live);
    });
    return () => {
      gone = true;
      dockRef.current = undefined;
      live?.unmount();
    };
  }, [children, gridPos, specs, storageKey, host, answer]);

  useEffect(() => {
    if (dock && titles.size) {
      dock.set_titles(titles);
    }
  }, [dock, titles]);

  // A slot is the panel's for the dock's lifetime, so each precompile miss compiles once per mount.
  const compiling = useRef(new Map<string, AbortController>());
  useEffect(() => {
    if (!dock) {
      return;
    }
    for (const p of children) {
      const key = p.state.key!;
      const spec = specs[key];
      const slot = slots.get(key);
      if (!('precompile' in spec) || !slot || compiling.current.has(key) || cached(spec.precompile)) {
        continue;
      }
      const abort = new AbortController();
      compiling.current.set(key, abort);
      compile(spec.precompile, p, slot, abort.signal).then(
        ({ w, h }) => dock.set_min(key, w, h),
        (e) => {
          if (!abort.signal.aborted) {
            throw e;
          }
        }
      );
    }
  }, [dock, children, specs, slots]);
  useEffect(() => {
    const live = compiling.current;
    return () => {
      live.forEach((a) => a.abort());
      live.clear();
    };
  }, [dock]);

  const closed = menu && dock ? children.filter((p) => !dock.hosted().includes(p.state.key!)) : [];
  return (
    <div className={styles.wrap} onPointerDownCapture={(e) => (pointer.current = { x: e.clientX, y: e.clientY })}>
      <div ref={root} className={styles.root} />
      {host && !answer && <div className={styles.waiting}>Waiting for the embedding page…</div>}
      {children.map((p) => {
        const slot = slots.get(p.state.key!);
        return (
          slot &&
          createPortal(
            <LazyLoader key={p.state.key!} className={styles.fill}>
              <p.Component model={p} />
            </LazyLoader>,
            slot,
            p.state.key
          )
        );
      })}
      {menu && dock && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(undefined)}
          renderMenuItems={() =>
            closed.length ? (
              closed.map((p) => (
                <Menu.Item key={p.state.key} label={titles.get(p.state.key!)!} onClick={() => dock.add_tab(menu.gid, p.state.key!)} />
              ))
            ) : (
              <Menu.Item label="Every panel is open" disabled />
            )
          }
        />
      )}
    </div>
  );
}

const getStyles = (theme: GrafanaTheme2) => ({
  wrap: css({
    position: 'relative',
    flex: '1 1 auto',
    minHeight: 0,
    '--tl-group-bg': theme.colors.background.primary,
    '--tl-tabstrip-bg': theme.colors.background.secondary,
    '--tl-tab-bg': theme.colors.background.secondary,
    '--tl-tab-border': theme.colors.border.weak,
    '--tl-tab-active-bg': theme.colors.background.primary,
    '--tl-tab-active-fg': theme.colors.text.primary,
    '--tl-fg': theme.colors.text.secondary,
    '--tl-content-pad': '0',
    '.tl-grafana-slot': { width: '100%', height: '100%' },
  }),
  root: css({ position: 'absolute', inset: 0 }),
  waiting: css({ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', color: theme.colors.text.secondary }),
  fill: css({ width: '100%', height: '100%' }),
});
