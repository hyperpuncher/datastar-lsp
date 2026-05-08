export function TestComponent() {
	return (
		<div data-signals:counter="0">
			{/* Signal definitions */}
			<input data-bind:search />
			<div data-computed:double="$counter * 2"></div>

			{/* Basic attributes */}
			<div data-show="$counter > 0">Visible when counter &gt; 0</div>
			<div data-text="$user?.name ?? 'Anonymous'"></div>
			<div data-attr:disabled="$counter > 10"></div>
			<div data-class:highlight="$counter % 2 === 0"></div>

			{/* Event handlers */}
			<button data-on:click="$counter++">Increment</button>
			<button
				data-on:click__debounce.500ms="$counter--"
			>
				Decrement (debounced)
			</button>

			{/* Backend actions */}
			<button data-on:click="@get('/api/data')">Fetch data</button>
			<button data-on:click="@post('/api/save', {id: $userId})">Save</button>
			<button
				data-on:click={`@get('/api/items?page=${page}')`}
			>
				Dynamic URL
			</button>

			{/* JSX-specific patterns */}
			<div
				attrs={{
					"data-on:click": "@get('/foo')",
					"data-show": "$visible",
				}}
			>
				Attrs object
			</div>

			{/* Conditional JSX */}
			{items.map(({ id, label, active }) => (
				<div
					key={id}
					data-bind:selected={id === current}
					data-class:active={active}
					data-text={label}
				/>
			))}

			{/* Forms */}
			<input
				type="radio"
				name="condition"
				data-bind:condition
				value="all"
				checked={id === "all"}
			/>

			{/* Pro attributes */}
			<div data-on-resize__debounce.10ms="$width = el.offsetWidth"></div>
			<div data-persist__session="{include: /user/}"></div>
			<div data-query-string__filter__history></div>
		</div>
	);
}
