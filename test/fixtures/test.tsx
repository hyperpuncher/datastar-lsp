// Same content as JSX, but with TypeScript types
interface Item {
	id: string;
	label: string;
	active: boolean;
}

export function TestComponent(props: { visible: boolean; userId: number }) {
	const page = 1;
	const current = "all";
	const items: Item[] = [];

	return (
		<div data-signals:counter={`${0}`}>
			{/* Signal definitions */}
			<input data-bind:search />
			<div data-computed:double="$counter * 2"></div>

			{/* Backend actions */}
			<button data-on:click="@get('/api/data')">Fetch data</button>
			<button data-on:click={`@post('/api/save', {id: ${props.userId}})`}>
				Save
			</button>

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

			{/* Event handlers */}
			<button data-on:click="$counter++">+</button>
			<button data-on:click__debounce.500ms="$counter--">-</button>

			{/* Various attributes */}
			<div data-show="$counter > 0"></div>
			<div data-text="$user?.name ?? 'N/A'"></div>
			<div data-attr:aria-label={`${$counter} items`}></div>
			<div data-class:active="$counter % 2 === 0"></div>

			{/* Pro */}
			<div data-persist="{include: /user/}"></div>
		</div>
	);
}
