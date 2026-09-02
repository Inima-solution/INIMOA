import type { NumberRangeFilter } from '@app/features/next-soup/filters/filter-store';
import { Button, Checkbox } from '@ui';
import { createEffect, createSignal, Show } from 'solid-js';
import { validateNumberRange } from './task-custom-property-filter';

type BoundMode = 'gt' | 'gte' | 'lt' | 'lte';

const readNumber = (value: string): number | undefined => {
  if (value.trim() === '') return;
  const number = Number(value);
  return Number.isFinite(number)
    ? Object.is(number, -0)
      ? 0
      : number
    : undefined;
};

export const TaskNumberRangeEditor = (props: {
  label: string;
  value?: NumberRangeFilter;
  exclude?: boolean;
  onApply: (range: NumberRangeFilter, exclude: boolean) => void;
  onClear: () => void;
}) => {
  const [lowerValue, setLowerValue] = createSignal('');
  const [upperValue, setUpperValue] = createSignal('');
  const [lowerMode, setLowerMode] = createSignal<BoundMode>('gte');
  const [upperMode, setUpperMode] = createSignal<BoundMode>('lte');
  const [exclude, setExclude] = createSignal(false);
  const [error, setError] = createSignal<string>();

  createEffect(() => {
    const value = props.value;
    const lower = value?.gt ?? value?.gte;
    const upper = value?.lt ?? value?.lte;
    setLowerValue(lower === undefined ? '' : String(lower));
    setUpperValue(upper === undefined ? '' : String(upper));
    setLowerMode(value?.gt !== undefined ? 'gt' : 'gte');
    setUpperMode(value?.lt !== undefined ? 'lt' : 'lte');
    setExclude(props.exclude === true);
    setError(undefined);
  });

  const apply = () => {
    const lower = readNumber(lowerValue());
    const upper = readNumber(upperValue());
    const range: NumberRangeFilter = {
      ...(lower !== undefined ? { [lowerMode()]: lower } : {}),
      ...(upper !== undefined ? { [upperMode()]: upper } : {}),
    };
    const nextError =
      (lowerValue().trim() !== '' && lower === undefined) ||
      (upperValue().trim() !== '' && upper === undefined)
        ? 'Bounds must be finite numbers.'
        : validateNumberRange(range);
    if (nextError) {
      setError(nextError);
      return;
    }
    setError(undefined);
    props.onApply(range, exclude());
  };

  return (
    <fieldset
      class="flex flex-col gap-2 p-2 text-sm text-ink"
      aria-label={`${props.label} number range`}
    >
      <div class="flex gap-2">
        <select
          aria-label={`${props.label} lower operator`}
          value={lowerMode()}
          onChange={(event) =>
            setLowerMode(event.currentTarget.value as BoundMode)
          }
          class="border border-edge-muted bg-surface px-1"
        >
          <option value="gte">≥</option>
          <option value="gt">&gt;</option>
        </select>
        <input
          aria-label={`${props.label} lower bound`}
          inputmode="decimal"
          value={lowerValue()}
          onInput={(event) => setLowerValue(event.currentTarget.value)}
          class="min-w-0 flex-1 border border-edge-muted bg-surface px-2 py-1"
        />
      </div>
      <div class="flex gap-2">
        <select
          aria-label={`${props.label} upper operator`}
          value={upperMode()}
          onChange={(event) =>
            setUpperMode(event.currentTarget.value as BoundMode)
          }
          class="border border-edge-muted bg-surface px-1"
        >
          <option value="lte">≤</option>
          <option value="lt">&lt;</option>
        </select>
        <input
          aria-label={`${props.label} upper bound`}
          inputmode="decimal"
          value={upperValue()}
          onInput={(event) => setUpperValue(event.currentTarget.value)}
          class="min-w-0 flex-1 border border-edge-muted bg-surface px-2 py-1"
        />
      </div>
      <Checkbox checked={exclude()} onChange={setExclude}>
        <Checkbox.Control />
        <Checkbox.Label>Exclude matching tasks</Checkbox.Label>
      </Checkbox>
      <Show when={error()}>
        {(message) => (
          <p role="alert" class="text-failure">
            {message()}
          </p>
        )}
      </Show>
      <div class="flex gap-2">
        <Button type="button" variant="cta" size="sm" onClick={apply}>
          Apply
        </Button>
        <Button
          type="button"
          variant="base"
          size="sm"
          onClick={() => props.onClear()}
        >
          Clear
        </Button>
      </div>
    </fieldset>
  );
};
