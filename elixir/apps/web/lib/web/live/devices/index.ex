defmodule Web.Devices.Index do
  use Web, :live_view

  alias Domain.Devices

  def mount(_params, _session, socket) do
    {_, devices} = Devices.list_devices(socket.assigns.subject, preload: :actor)

    {:ok, assign(socket, devices: devices)}
  end

  def render(assigns) do
    ~H"""
    <.breadcrumbs home_path={~p"/#{@account}/dashboard"}>
      <.breadcrumb path={~p"/#{@account}/devices"}>Devices</.breadcrumb>
    </.breadcrumbs>
    <.header>
      <:title>
        All devices
      </:title>
    </.header>
    <!-- Devices Table -->
    <div class="bg-white dark:bg-gray-800 overflow-hidden">
      <.resource_filter />
      <.table id="devices" rows={@devices} row_id={&"device-#{&1.id}"}>
        <:col :let={device} label="CLIENT" sortable="true">
          <.link
            navigate={~p"/#{@account}/devices/#{device.id}"}
            class="font-medium text-blue-600 dark:text-blue-500 hover:underline"
          >
            <%= device.name %>
          </.link>
        </:col>
        <:col :let={device} label="USER" sortable="true">
          <.link
            navigate={~p"/#{@account}/actors/#{device.actor.id}"}
            class="font-medium text-blue-600 dark:text-blue-500 hover:underline"
          >
            <%= device.actor.name %>
          </.link>
        </:col>
        <:col :let={_device} label="STATUS" sortable="true">
          <.badge type="success">
            TODO: Online
          </.badge>
        </:col>
        <:action :let={device}>
          <.link
            navigate={~p"/#{@account}/devices/#{device.id}"}
            class="block py-2 px-4 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-white"
          >
            Show
          </.link>
        </:action>
        <:action>
          <a
            href="#"
            class="block py-2 px-4 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-white"
          >
            TODO: Archive
          </a>
        </:action>
      </.table>
      <.paginator page={3} total_pages={100} collection_base_path={~p"/#{@account}/devices"} />
    </div>
    """
  end

  defp resource_filter(assigns) do
    ~H"""
    <div class="flex flex-col md:flex-row items-center justify-between space-y-3 md:space-y-0 md:space-x-4 p-4">
      <div class="w-full md:w-1/2">
        <form class="flex items-center">
          <label for="simple-search" class="sr-only">Search</label>
          <div class="relative w-full">
            <div class="absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none">
              <.icon name="hero-magnifying-glass" class="w-5 h-5 text-gray-500 dark:text-gray-400" />
            </div>
            <input
              type="text"
              id="simple-search"
              class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-500 focus:border-primary-500 block w-full pl-10 p-2 dark:bg-gray-700 dark:border-gray-600 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500"
              placeholder="Search"
              required=""
            />
          </div>
        </form>
      </div>
      <.button_group>
        <:first>
          All
        </:first>
        <:middle>
          Online
        </:middle>
        <:last>
          Archived
        </:last>
      </.button_group>
    </div>
    """
  end
end