# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Object do
  let(:state) { Inkoc::State.new(Inkoc::Config.new) }

  describe '#object?' do
    it 'returns true' do
      expect(described_class.new).to be_object
    end
  end

  describe '#type_compatible?' do
    context 'when comparing with a Dynamic' do
      it 'returns true' do
        ours = described_class.new
        theirs = Inkoc::TypeSystem::Dynamic.new

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing with an Optional' do
      it 'returns true if we are compatible with the wrapped type' do
        proto = described_class.new
        ours = described_class.new(prototype: proto)
        theirs = Inkoc::TypeSystem::Optional.new(proto)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true when not compatible but we implement marker::Optional' do
        ours = described_class.new
        theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

        allow(ours)
          .to receive(:optional_marker_implemented?)
          .with(state)
          .and_return(true)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns false when the objects are not compatible' do
        ours = described_class.new
        theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end

    context 'when comparing with a Trait' do
      it 'returns true if the trait has been implemented' do
        ours = described_class.new
        theirs = Inkoc::TypeSystem::Trait.new

        ours.implement_trait(theirs)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns false if the trait has not been implemented' do
        ours = described_class.new
        theirs = Inkoc::TypeSystem::Trait.new

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end

    context 'when comparing with a regular object' do
      it 'returns true if the object resides in our prototype chain' do
        theirs = described_class.new
        ours = described_class.new(prototype: theirs)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true when both objects implement the Compatible trait' do
        theirs = described_class.new
        ours = described_class.new

        allow(ours)
          .to receive(:implements_compatible_marker?)
          .and_return(true)

        allow(theirs)
          .to receive(:implements_compatible_marker?)
          .and_return(true)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns false when the object is not in the prototype chain' do
        theirs = described_class.new
        ours = described_class.new

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end

    context 'when comparing with a type instance of a compatible object' do
      it 'returns true' do
        theirs = described_class.new
        ours = described_class.new(prototype: theirs)

        expect(ours.type_compatible?(theirs.new_instance, state)).to eq(true)
      end
    end

    context 'when comparing with a generic object' do
      it 'returns true when two objects are compatible' do
        int_type = state.typedb.integer_type

        ours = state.typedb
          .new_array_of_type(state.typedb.new_array_of_type(int_type))

        theirs = state.typedb
          .new_array_of_type(state.typedb.new_array_of_type(int_type))

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true when a type is initialised with its own parameters' do
        param = state
          .typedb
          .array_type
          .lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

        ours = state.typedb.new_array_of_type(param)

        theirs = state
          .typedb
          .new_array_of_type(state.typedb.integer_type.new_instance)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true when an array is compatible with an array of traits' do
        trait = Inkoc::TypeSystem::Trait.new(name: 'A')
        int_type = state.typedb.integer_type

        int_type.implement_trait(trait)

        ours = state.typedb
          .new_array_of_type(state.typedb.new_array_of_type(int_type))

        theirs = state.typedb
          .new_array_of_type(state.typedb.new_array_of_type(trait))

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      # rubocop: disable Metrics/LineLength
      it 'returns true when comparing an initialised object with an uninitialised one' do
        trait = Inkoc::TypeSystem::Trait.new(name: 'A')
        ours = state.typedb.array_type

        theirs = state.typedb
          .new_array_of_type(state.typedb.new_array_of_type(trait))

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end
      # rubocop: enable Metrics/LineLength

      it 'returns false when the objects are not compatible' do
        ours = state.typedb.new_array_of_type(
          state.typedb.new_array_of_type(state.typedb.integer_type)
        )

        theirs = state.typedb.new_array_of_type(
          state.typedb.new_array_of_type(state.typedb.float_type)
        )

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end
  end

  describe '#implements_compatible_marker?' do
    it 'returns true when the Compatible marker is implemented' do
      object = described_class.new

      allow(object)
        .to receive(:marker_implemented?)
        .with(Inkoc::Config::COMPATIBLE_CONST, state)
        .and_return(true)

      expect(object.implements_compatible_marker?(state)).to eq(true)
    end

    it 'returns false when the Compatible marker is not implemented' do
      object = described_class.new

      expect(object.implements_compatible_marker?(state)).to eq(false)
    end
  end

  describe '#compatible_with_optional?' do
    it 'returns true if we are compatible with the wrapped type' do
      proto = described_class.new
      ours = described_class.new(prototype: proto)
      theirs = Inkoc::TypeSystem::Optional.new(proto)

      expect(ours.compatible_with_optional?(theirs, state)).to eq(true)
    end

    it 'returns true when not compatible but we implement marker::Optional' do
      ours = described_class.new
      theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

      allow(ours)
        .to receive(:optional_marker_implemented?)
        .with(state)
        .and_return(true)

      expect(ours.compatible_with_optional?(theirs, state)).to eq(true)
    end

    it 'returns false when the objects are not compatible' do
      ours = described_class.new
      theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

      expect(ours.compatible_with_optional?(theirs, state)).to eq(false)
    end
  end

  describe '#optional_marker_implemented?' do
    let(:object) { described_class.new }

    it 'returns true when the Optional marker is implemented' do
      allow(object)
        .to receive(:marker_implemented?)
        .with(Inkoc::Config::OPTIONAL_CONST, state)
        .and_return(true)

      expect(object.optional_marker_implemented?(state)).to eq(true)
    end

    it 'returns false when the Optional marker is not implemented' do
      allow(object)
        .to receive(:marker_implemented?)
        .with(Inkoc::Config::OPTIONAL_CONST, state)
        .and_return(false)

      expect(object.optional_marker_implemented?(state)).to eq(false)
    end
  end

  describe '#marker_implemented?' do
    let(:object) { described_class.new }
    let(:marker) { Inkoc::TypeSystem::Trait.new }

    it 'returns true if a marker is implemented' do
      allow(state)
        .to receive(:type_of_module_global)
        .with(Inkoc::Config::MARKER_MODULE, 'foo')
        .and_return(marker)

      object.implement_trait(marker)

      expect(object.marker_implemented?('foo', state)).to eq(true)
    end

    it 'returns false if a marker is not implemented' do
      allow(state)
        .to receive(:type_of_module_global)
        .with(Inkoc::Config::MARKER_MODULE, 'foo')
        .and_return(marker)

      expect(object.marker_implemented?('foo', state)).to eq(false)
    end

    it 'returns false if a marker does not exist' do
      allow(state)
        .to receive(:type_of_module_global)
        .with(Inkoc::Config::MARKER_MODULE, 'foo')
        .and_return(nil)

      expect(object.marker_implemented?('foo', state)).to eq(false)
    end
  end

  describe '#compatible_with_trait?' do
    let(:object) { described_class.new }
    let(:trait) { Inkoc::TypeSystem::Trait.new }

    it 'returns true when the trait is implemented' do
      object.implement_trait(trait)

      expect(object.compatible_with_trait?(trait)).to eq(true)
    end

    it 'returns true when the trait is an instance of an implemented trait' do
      object.implement_trait(trait)

      expect(object.compatible_with_trait?(trait.new_instance)).to eq(true)
    end

    it 'returns false when the trait is not implemented' do
      expect(object.compatible_with_trait?(trait)).to eq(false)
    end
  end

  describe '#prototype_chain_compatible?' do
    it 'returns true when our prototype equals the given object' do
      theirs = described_class.new
      ours = described_class.new(prototype: theirs)

      expect(ours.prototype_chain_compatible?(theirs)).to eq(true)
    end

    it 'returns false when our prototype chain does not include the object' do
      ours = described_class.new
      theirs = described_class.new

      expect(ours.prototype_chain_compatible?(theirs)).to eq(false)
    end
  end

  describe '#lookup_method' do
    it 'returns a Symbol' do
      object = described_class.new
      method = described_class.new

      object.define_attribute('foo', method)

      symbol = object.lookup_method('foo')

      expect(symbol).to be_an_instance_of(Inkoc::Symbol)
      expect(symbol.type).to eq(method)
    end
  end

  describe '#responds_to_message' do
    it 'returns true for a defined message' do
      object = described_class.new
      method = described_class.new

      object.define_attribute('foo', method)

      expect(object.responds_to_message?('foo')).to eq(true)
    end

    it 'returns false for an undefined message' do
      object = described_class.new

      expect(object.responds_to_message?('foo')).to eq(false)
    end
  end

  describe '#initialize_type_parameters_in_order' do
    it 'initializes the parameters in order' do
      object = described_class.new
      instance1 = described_class.new
      instance2 = described_class.new
      param1 = object.define_type_parameter('A')
      param2 = object.define_type_parameter('B')

      object.initialize_type_parameters_in_order([instance1, instance2])

      expect(object.lookup_type_parameter_instance(param1)).to eq(instance1)
      expect(object.lookup_type_parameter_instance(param2)).to eq(instance2)
    end
  end

  describe '#compatible_with_type_parameter' do
    let(:object) { described_class.new }
    let(:trait) { Inkoc::TypeSystem::Trait.new }

    let(:type_param) do
      Inkoc::TypeSystem::TypeParameter
        .new(name: 'A', required_traits: [trait])
    end

    it 'returns true when we are compatible with a type parameter' do
      object.implement_trait(trait)

      expect(object.compatible_with_type_parameter?(type_param, state))
        .to eq(true)
    end

    it 'returns false when we are not compatible with a type parameter' do
      expect(object.compatible_with_type_parameter?(type_param, state))
        .to eq(false)
    end
  end

  describe '#new_instance' do
    it 'returns a copy with a fresh list of type parameter instances' do
      original = described_class.new
      param = original.define_type_parameter('T')

      original.initialize_type_parameter(param, described_class.new)

      new_instance = original.new_instance

      expect(new_instance.type_parameter_instances).to be_empty
    end

    it 'sets the prototype of the new instance' do
      object = described_class.new(name: 'A')
      instance = object.new_instance

      expect(instance.prototype).to eq(object)
      expect(instance.new_instance.prototype).to eq(object)
    end
  end

  describe '#type_instance_of?' do
    it 'returns true when a type is an instance of another type' do
      base = described_class.new(name: 'A')
      base.define_type_parameter('t')

      expect(base).to be_type_instance_of(base)
      expect(base.new_instance).to be_type_instance_of(base)
    end

    it 'returns false when a type is not a type instance of another type' do
      foo = described_class.new(name: 'A')
      bar = described_class.new(name: 'B')

      foo.define_type_parameter('t')

      expect(bar).not_to be_type_instance_of(foo)
    end
  end

  describe '#message_return_type' do
    it 'returns a fully resolved return type' do
      source = described_class.new(name: 'A')
      method = Inkoc::TypeSystem::Block.new(name: 'foo')
      self_type = described_class.new(name: 'B')
      return_type = described_class.new(name: 'C')
      instance = described_class.new(name: 'D')

      param_a = self_type.define_type_parameter('A')
      param_b = return_type.define_type_parameter('B')

      return_type.initialize_type_parameter(param_b, param_a)
      self_type.initialize_type_parameter(param_a, instance)

      method.return_type = return_type
      source.define_attribute('foo', method)

      type = source.message_return_type('foo', self_type)

      # Example: def foo -> A!(B) where B is initialised in "self" to Integer.
      # In this case the return type should be A!(Integer).
      expect(type).to be_type_instance_of(return_type)
      expect(type.lookup_type_parameter_instance(param_b)).to eq(instance)
    end
  end

  describe '#generic_object' do
    it 'returns true when type parameters are defined' do
      obj = described_class.new
      obj.define_type_parameter('T')

      expect(obj).to be_generic_object
    end

    it 'returns false when no type parameters are defined' do
      obj = described_class.new

      expect(obj).not_to be_generic_object
    end
  end

  describe '#initialize_as' do
    let(:block_type) do
      Inkoc::TypeSystem::Block.new(name: 'foo').tap do |block|
        block.define_type_parameter('T')
      end
    end

    let(:self_type) do
      described_class.new(name: 'A').tap do |object|
        object.define_type_parameter('X')
      end
    end

    context 'when initialising as an generic object' do
      it 'initialises any type parameters defined in the block' do
        int_type = state.typedb.integer_type
        param = block_type.lookup_type_parameter('T')

        to_init = state.typedb.new_array_of_type(param)
        init_as = state.typedb.new_array_of_type(int_type)

        to_init.initialize_as(init_as, block_type, self_type)

        expect(block_type.lookup_type_parameter_instance(param)).to eq(int_type)
        expect(self_type.lookup_type_parameter_instance(param)).to be_nil
      end

      it 'initialises any type parameters defined in the self type' do
        int_type = state.typedb.integer_type
        param = self_type.lookup_type_parameter('X')

        to_init = state.typedb.new_array_of_type(param)
        init_as = state.typedb.new_array_of_type(int_type)

        to_init.initialize_as(init_as, block_type, self_type)

        expect(block_type.lookup_type_parameter_instance(param)).to be_nil
        expect(self_type.lookup_type_parameter_instance(param)).to eq(int_type)
      end
    end

    context 'when initialising as a type parameter' do
      it 'does nothing' do
        param = self_type.lookup_type_parameter('X')
        to_init = state.typedb.new_array_of_type(param)

        to_init.initialize_as(param, block_type, self_type)

        expect(block_type.lookup_type_parameter_instance(param)).to be_nil
        expect(self_type.lookup_type_parameter_instance(param)).to be_nil
      end
    end
  end

  describe '#implements_method?' do
    it 'returns true for a method that is implemented' do
      object = described_class.new
      method1 = Inkoc::TypeSystem::Block.new(name: 'foo')
      method2 = Inkoc::TypeSystem::Block.new(name: 'foo')

      object.define_attribute('foo', method1)

      expect(object.implements_method?(method2, state)).to eq(true)
    end

    it 'returns false for a method that is not implemented' do
      object = described_class.new
      method = Inkoc::TypeSystem::Block.new(name: 'foo')

      expect(object.implements_method?(method, state)).to eq(false)
    end
  end

  describe '#reassign_attribute' do
    it 'reassigns an existing attribute' do
      object = described_class.new
      old_type = described_class.new(name: 'A')
      new_type = described_class.new(name: 'B')

      object.define_attribute('name', old_type)
      object.reassign_attribute('name', new_type)

      expect(object.attributes['name'].type).to eq(new_type)
    end
  end

  describe '#lookup_method' do
    it 'supports looking up a method from an implemented trait' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      method = Inkoc::TypeSystem::Block
        .named_method('inspect', state.typedb.block_type)

      trait.define_attribute(method.name, method)

      object = described_class.new(name: 'Person')

      object.implement_trait(trait)

      expect(object.lookup_method(method.name).type).to eq(method)
    end

    it 'supports looking up a required method from an implemented trait' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      method = Inkoc::TypeSystem::Block
        .named_method('inspect', state.typedb.block_type)

      trait.define_required_method(method)

      object = described_class.new(name: 'Person')

      object.implement_trait(trait)

      expect(object.lookup_method(method.name).type).to eq(method)
    end
  end

  describe '#implement_trait' do
    it 'implements a trait' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      object = described_class.new

      object.implement_trait(trait)

      expect(object.implemented_traits.values.first).to eq(trait)
    end
  end

  describe '#implements_trait?' do
    it 'returns true when a trait is implemented' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect', unique_id: 1)
      object = described_class.new

      object.implement_trait(trait)

      expect(object.implements_trait?(trait)).to eq(true)
    end

    it 'returns true when using an uninitialised trait' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect', unique_id: 1)
      trait.define_type_parameter('T')

      object = described_class.new

      # When using "Self" in a trait the type returned is an uninitialised
      # version of the trait. This is OK in the trait itself, but requires some
      # extra care when checking if an object implements said trait.
      object.implement_trait(trait.new_instance([state.typedb.integer_type]))

      expect(object.implements_trait?(trait)).to eq(true)
    end

    it 'returns true when the prototype implements the trait' do
      parent = described_class.new(name: 'A')
      child = described_class.new(name: 'B', prototype: parent)
      trait = Inkoc::TypeSystem::Trait.new(name: 'C', unique_id: 1)

      parent.implement_trait(trait)

      expect(child.implements_trait?(trait)).to eq(true)
    end

    it 'returns false when a trait is not implemented' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      object = described_class.new

      expect(object.implements_trait?(trait)).to eq(false)
    end
  end

  describe '#remove_trait_implementation' do
    it 'removes the implementation of a trait' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      object = described_class.new

      object.implement_trait(trait)
      object.remove_trait_implementation(trait)

      expect(object.implemented_traits).to be_empty
    end
  end

  describe '#without_empty_type_parameters' do
    let(:array) do
      param = Inkoc::TypeSystem::TypeParameter.new(name: 'A')

      state.typedb.new_array_of_type(param)
    end

    it 'removes all empty type parameter instances' do
      copied = array.without_empty_type_parameters

      expect(copied.type_parameter_instances).to be_empty
    end

    it 'removes empty type parameter instances in nested types' do
      outer_array = state.typedb.new_array_of_type(array)
      new_outer = outer_array.without_empty_type_parameters

      expect(new_outer.type_parameter_instances).not_to be_empty

      param = new_outer
        .lookup_type_parameter(Inkoc::Config::ARRAY_TYPE_PARAMETER)

      instance = new_outer.lookup_type_parameter_instance(param)

      expect(instance.type_parameter_instances).to be_empty
    end
  end

  describe '#guard_unknown_message?' do
    it 'returns true for an undefined message' do
      object = described_class.new

      expect(object.guard_unknown_message?('foo')).to eq(true)
    end

    it 'returns false for a defined message' do
      object = described_class.new
      method = Inkoc::TypeSystem::Block.new(name: 'foo')

      object.attributes.define(method.name, method)

      expect(object.guard_unknown_message?('foo')).to eq(false)
    end
  end

  describe '#initialize_type_parameter?' do
    let(:object) { described_class.new }

    it 'returns false for a type parameter not owned by the object' do
      param = Inkoc::TypeSystem::TypeParameter.new(name: 'A')

      expect(object.initialize_type_parameter?(param)).to eq(false)
    end

    it 'returns false for a type parameter that is already initialised' do
      param = object.define_type_parameter('A')

      object.initialize_type_parameter(param, state.typedb.integer_type)

      expect(object.initialize_type_parameter?(param)).to eq(false)
    end

    it 'returns true for a parameter that is initialised with a parameter' do
      param1 = object.define_type_parameter('A')
      param2 = object.define_type_parameter('B')

      object.initialize_type_parameter(param1, param2)

      expect(object.initialize_type_parameter?(param1)).to eq(true)
    end

    it 'returns true when a parameter is initialised to itself' do
      param = object.define_type_parameter('A')

      object.initialize_type_parameter(param, param)

      expect(object.initialize_type_parameter?(param)).to eq(true)
    end
  end
end
