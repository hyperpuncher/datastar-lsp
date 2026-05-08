<div>
	{{-- Signal definitions --}}
	<div data-signals:counter="{{ 0 }}"></div>
	<input data-bind:search />

	{{-- Basic attributes --}}
	<div data-show="{{ $visible }}">Visible</div>
	<div data-text="{{ $username }}"></div>
	<div data-class:highlight="{{ $count % 2 === 0 }}"></div>

	{{-- Event handlers --}}
	<button data-on:click="$counter++">Increment</button>
	<button data-on:click__debounce.500ms="$counter--">Decrement</button>

	{{-- Backend actions --}}
	<button data-on:click="@get('/api/data')">Fetch</button>
	<button data-on:click="@post('/api/save')">Save</button>

	{{-- Form bindings --}}
	@foreach ($items as $item)
		<div data-bind:selected="{{ $item->id }}">
			<span data-text="{{ $item->label }}"></span>
		</div>
	@endforeach

	{{-- Modifiers --}}
	<div data-signals:my-var__case.kebab="'value'"></div>
</div>
