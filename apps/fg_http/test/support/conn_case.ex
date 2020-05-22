defmodule FgHttpWeb.ConnCase do
  @moduledoc """
  This module defines the test case to be used by
  tests that require setting up a connection.

  Such tests rely on `Phoenix.ConnTest` and also
  import other functionality to make it easier
  to build common data structures and query the data layer.

  Finally, if the test case interacts with the database,
  we enable the SQL sandbox, so changes done to the database
  are reverted at the end of every test. If you are using
  PostgreSQL, you can even run database tests asynchronously
  by setting `use FgHttpWeb.ConnCase, async: true`, although
  this option is not recommended for other databases.
  """

  use ExUnit.CaseTemplate

  alias Ecto.Adapters.SQL.Sandbox

  alias FgHttp.Fixtures

  using do
    quote do
      # Import conveniences for testing with connections
      import Plug.Conn
      import Phoenix.ConnTest
      alias FgHttpWeb.Router.Helpers, as: Routes

      # The default endpoint for testing
      @endpoint FgHttpWeb.Endpoint
    end
  end

  def new_conn do
    Phoenix.ConnTest.build_conn()
  end

  def authed_conn do
    user = Fixtures.user()

    session =
      Fixtures.session(%{
        user_id: user.id,
        user_password: "test",
        user_email: "test"
      })

    new_conn()
    |> Plug.Conn.assign(:current_user, user)
    |> Plug.Conn.assign(:current_session, session)
    |> Plug.Conn.assign(:user_signed_in?, true)
  end

  setup tags do
    :ok = Sandbox.checkout(FgHttp.Repo)

    unless tags[:async] do
      Sandbox.mode(FgHttp.Repo, {:shared, self()})
    end

    {:ok, unauthed_conn: new_conn(), authed_conn: authed_conn()}
  end
end
